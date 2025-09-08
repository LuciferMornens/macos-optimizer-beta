/**
 * Operation Queue - Manages concurrent operations and debouncing
 * Prevents duplicate operations and manages system resource usage
 */
class OperationQueue {
    constructor(maxConcurrent = 3) {
        this.queue = [];
        this.running = new Map();
        this.maxConcurrent = maxConcurrent;
        this.debounceTimers = new Map();
        this.operationHistory = [];
        this.maxHistory = 50;
    }
    
    /**
     * Add an operation to the queue with optional debouncing
     */
    async add(operation, options = {}) {
        const { 
            debounce = 0, 
            priority = 0, 
            id = null, 
            description = 'Operation',
            timeout = 30000 
        } = options;
        
        // Debounce if needed
        if (debounce > 0 && id) {
            if (this.debounceTimers.has(id)) {
                clearTimeout(this.debounceTimers.get(id));
            }
            
            return new Promise((resolve, reject) => {
                const timer = setTimeout(() => {
                    this.debounceTimers.delete(id);
                    this.enqueue(operation, priority, description, timeout).then(resolve).catch(reject);
                }, debounce);
                this.debounceTimers.set(id, timer);
            });
        }
        
        return this.enqueue(operation, priority, description, timeout);
    }
    
    /**
     * Enqueue an operation for execution
     */
    async enqueue(operation, priority, description, timeout) {
        return new Promise((resolve, reject) => {
            const queueItem = {
                operation,
                priority,
                description,
                timeout,
                resolve,
                reject,
                enqueuedAt: Date.now()
            };
            
            this.queue.push(queueItem);
            this.queue.sort((a, b) => b.priority - a.priority);
            this.processQueue();
        });
    }
    
    /**
     * Process the queue - execute operations up to maxConcurrent limit
     */
    async processQueue() {
        while (this.queue.length > 0 && this.running.size < this.maxConcurrent) {
            const item = this.queue.shift();
            const operationId = this.generateOperationId();
            
            this.running.set(operationId, item);
            
            // Execute operation with timeout
            this.executeWithTimeout(operationId, item);
        }
    }
    
    /**
     * Execute operation with timeout handling
     */
    async executeWithTimeout(operationId, item) {
        const { operation, timeout, resolve, reject, description, enqueuedAt } = item;
        const startTime = Date.now();
        const waitTime = startTime - enqueuedAt;
        
        try {
            console.log(`Executing operation: ${description} (waited ${waitTime}ms)`);
            
            // Create timeout promise
            const timeoutPromise = new Promise((_, timeoutReject) => {
                setTimeout(() => {
                    timeoutReject(new Error(`Operation timeout after ${timeout}ms`));
                }, timeout);
            });
            
            // Race between operation and timeout
            const result = await Promise.race([
                operation(),
                timeoutPromise
            ]);
            
            const duration = Date.now() - startTime;
            console.log(`Operation completed: ${description} (${duration}ms)`);
            
            this.addToHistory(description, true, duration, waitTime);
            resolve(result);
            
        } catch (error) {
            const duration = Date.now() - startTime;
            console.error(`Operation failed: ${description} (${duration}ms):`, error);
            
            this.addToHistory(description, false, duration, waitTime, error.message);
            reject(error);
            
        } finally {
            this.running.delete(operationId);
            this.processQueue(); // Process next items in queue
        }
    }
    
    /**
     * Add operation to history
     */
    addToHistory(description, success, duration, waitTime, error = null) {
        const historyItem = {
            description,
            success,
            duration,
            waitTime,
            error,
            timestamp: new Date().toISOString(),
            displayTime: new Date().toLocaleTimeString()
        };
        
        this.operationHistory.unshift(historyItem);
        if (this.operationHistory.length > this.maxHistory) {
            this.operationHistory.pop();
        }
        
        // Update history UI if available
        this.updateHistoryUI();
    }
    
    /**
     * Update history UI
     */
    updateHistoryUI() {
        const historyContainer = document.getElementById('operation-history-list');
        if (!historyContainer) return;
        
        const recentItems = this.operationHistory.slice(0, 10);
        historyContainer.innerHTML = recentItems.map(item => `
            <div class="history-item ${item.success ? 'success' : 'error'}">
                <div class="history-operation">${item.description}</div>
                <div class="history-meta">
                    <span class="history-time">${item.displayTime}</span>
                    <span class="history-duration">${item.duration}ms</span>
                    ${item.waitTime > 100 ? `<span class="history-wait">+${item.waitTime}ms wait</span>` : ''}
                </div>
                ${item.error ? `<div class="history-error">${item.error}</div>` : ''}
            </div>
        `).join('');
    }
    
    /**
     * Generate unique operation ID
     */
    generateOperationId() {
        return `op_${Date.now()}_${Math.random().toString(36).substr(2, 5)}`;
    }
    
    /**
     * Get queue status
     */
    getStatus() {
        return {
            queued: this.queue.length,
            running: this.running.size,
            maxConcurrent: this.maxConcurrent,
            totalOperations: this.operationHistory.length
        };
    }
    
    /**
     * Clear debounce timers for a specific operation
     */
    cancelDebounce(operationId) {
        if (this.debounceTimers.has(operationId)) {
            clearTimeout(this.debounceTimers.get(operationId));
            this.debounceTimers.delete(operationId);
            return true;
        }
        return false;
    }
    
    /**
     * Get operation history
     */
    getHistory() {
        return [...this.operationHistory];
    }
    
    /**
     * Clear operation history
     */
    clearHistory() {
        this.operationHistory = [];
        this.updateHistoryUI();
    }
    
    /**
     * Check if a specific operation is currently running
     */
    isOperationRunning(description) {
        return Array.from(this.running.values()).some(item => 
            item.description === description
        );
    }
    
    /**
     * Wait for all current operations to complete
     */
    async waitForAll() {
        const runningPromises = Array.from(this.running.entries()).map(([id, item]) => {
            return new Promise((resolve) => {
                const originalResolve = item.resolve;
                const originalReject = item.reject;
                
                item.resolve = (result) => {
                    originalResolve(result);
                    resolve();
                };
                
                item.reject = (error) => {
                    originalReject(error);
                    resolve();
                };
            });
        });
        
        await Promise.all(runningPromises);
    }
}

// Create global operation queue instance
const operationQueue = new OperationQueue(3);

// Make it available globally
window.operationQueue = operationQueue;

// Debug helpers
window.getQueueStatus = () => operationQueue.getStatus();
window.getOperationHistory = () => operationQueue.getHistory();

console.log('Operation Queue initialized with max', operationQueue.maxConcurrent, 'concurrent operations');