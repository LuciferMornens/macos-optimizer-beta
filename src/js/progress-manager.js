/**
 * Progress Manager - Handles real-time progress reporting for operations
 * Manages progress bars, events, and user feedback during long-running operations
 */
class ProgressManager {
    constructor() {
        this.activeOperations = new Map();
        this.progressBars = new Map();
        this.eventListenersSetup = false;
        this.setupEventListeners();
    }
    
    async setupEventListeners() {
        if (this.eventListenersSetup) return;
        
        try {
            // Wait for Tauri to be available
            while (!window.__TAURI__?.event) {
                await new Promise(resolve => setTimeout(resolve, 100));
            }
            
            console.log('Setting up progress event listeners...');
            
            await window.__TAURI__.event.listen('operation:start', (event) => {
                console.log('Operation started:', event.payload);
                this.onOperationStart(event.payload);
            });
            
            await window.__TAURI__.event.listen('progress:update', (event) => {
                console.log('Progress update:', event.payload);
                this.onProgressUpdate(event.payload);
            });
            
            await window.__TAURI__.event.listen('operation:complete', (event) => {
                console.log('Operation completed:', event.payload);
                this.onOperationComplete(event.payload);
            });
            
            this.eventListenersSetup = true;
            console.log('Progress event listeners setup complete');
        } catch (error) {
            console.error('Failed to setup progress event listeners:', error);
        }
    }
    
    onOperationStart(payload) {
        const { operation_id, operation_type, estimated_duration } = payload;
        
        this.activeOperations.set(operation_id, {
            type: operation_type,
            startTime: Date.now(),
            estimatedDuration: estimated_duration,
        });
        
        // Show activity indicator
        this.showGlobalActivity(this.getOperationDisplayName(operation_type));
        
        // Create progress bar if container exists
        this.createProgressBarForOperation(operation_id, operation_type);
    }
    
    onProgressUpdate(payload) {
        const { operation_id, progress, message, stage, can_cancel, eta_ms, throughput } = payload;
        
        // Update progress bar
        this.updateProgress(operation_id, progress, message, stage, eta_ms, throughput);
        
        // Update global activity message
        const operation = this.activeOperations.get(operation_id);
        if (operation) {
            this.showGlobalActivity(message);
        }
    }
    
    onOperationComplete(payload) {
        const { operation_id, success, message, duration } = payload;
        
        // Clean up operation
        this.activeOperations.delete(operation_id);
        
        // Remove progress bar after a delay
        setTimeout(() => {
            const progressBar = this.progressBars.get(operation_id);
            if (progressBar) {
                progressBar.classList.add('fade-out');
                setTimeout(() => {
                    progressBar.remove();
                    this.progressBars.delete(operation_id);
                }, 300);
            }
        }, 2000);
        
        // Hide global activity if no more operations
        if (this.activeOperations.size === 0) {
            this.hideGlobalActivity();
        }
        
        // Show completion notification
        if (window.showNotification) {
            window.showNotification(message, success ? 'success' : 'error');
        }
    }
    
    createProgressBarForOperation(operationId, operationType) {
        // Try to find appropriate container based on operation type
        let container = this.getContainerForOperation(operationType);
        
        if (!container) {
            // Create or use global progress container
            container = this.getOrCreateGlobalProgressContainer();
        }
        
        const progressBar = this.createProgressBar(operationId, container);
        const title = progressBar.querySelector('.progress-title');
        if (title) {
            title.textContent = this.getOperationDisplayName(operationType);
        }
    }
    
    getContainerForOperation(operationType) {
        switch (operationType) {
            case 'memory_optimization':
            case 'memory_optimization_admin':
                return document.getElementById('memory-progress-container') || 
                       document.querySelector('#memory .tab-content');
            case 'file_scan':
                return document.getElementById('scan-progress') || 
                       document.querySelector('#storage .tab-content');
            default:
                return null;
        }
    }
    
    getOrCreateGlobalProgressContainer() {
        let container = document.getElementById('global-progress-container');
        if (!container) {
            container = document.createElement('div');
            container.id = 'global-progress-container';
            container.className = 'global-progress-container';
            document.body.appendChild(container);
        }
        return container;
    }
    
    getOperationDisplayName(operationType) {
        const names = {
            'memory_optimization': 'Memory Optimization',
            'memory_optimization_admin': 'Deep Memory Clean',
            'file_scan': 'File System Scan',
            'file_clean': 'File Cleaning',
            'process_kill': 'Process Management'
        };
        return names[operationType] || 'System Operation';
    }
    
    createProgressBar(operationId, container) {
        const progressBar = document.createElement('div');
        progressBar.className = 'operation-progress';
        progressBar.dataset.operationId = operationId;
        progressBar.innerHTML = `
            <div class="progress-header">
                <span class="progress-title">Processing...</span>
                <button class="progress-cancel btn-link" data-operation="${operationId}" style="display: none;">
                    Cancel
                </button>
            </div>
            <div class="progress-bar-container">
                <div class="progress-bar-fill"></div>
                <div class="progress-percentage">0%</div>
            </div>
            <div class="progress-message">Starting...</div>
            <div class="progress-stage"></div>
        `;
        
        // Add cancel handler
        const cancelBtn = progressBar.querySelector('.progress-cancel');
        if (cancelBtn) {
            cancelBtn.addEventListener('click', () => {
                this.cancelOperation(operationId);
            });
        }
        
        container.appendChild(progressBar);
        this.progressBars.set(operationId, progressBar);
        
        // Animate in
        setTimeout(() => {
            progressBar.classList.add('animate-slideInUp');
        }, 10);
        
        return progressBar;
    }
    
    updateProgress(operationId, progress, message, stage, etaMs, throughput) {
        const bar = this.progressBars.get(operationId);
        if (!bar) return;
        
        const fill = bar.querySelector('.progress-bar-fill');
        const messageEl = bar.querySelector('.progress-message');
        const stageEl = bar.querySelector('.progress-stage');
        const percentageEl = bar.querySelector('.progress-percentage');
        const cancelBtn = bar.querySelector('.progress-cancel');
        
        if (fill) {
            fill.style.width = `${Math.max(0, Math.min(100, progress))}%`;
            fill.style.transition = 'width 0.3s ease-out';
        }
        
        if (percentageEl) {
            percentageEl.textContent = `${Math.round(progress)}%`;
        }
        
        if (messageEl) {
            messageEl.textContent = message;
        }
        
        if (stageEl) {
            let extra = '';
            if (etaMs !== undefined && etaMs !== null && etaMs > 0) {
                const secs = Math.round(etaMs / 1000);
                extra += ` • ETA: ${secs}s`;
            }
            if (throughput && (throughput.files_per_s || throughput.mb_per_s)) {
                const fps = throughput.files_per_s ? `${throughput.files_per_s.toFixed(1)} f/s` : '';
                const mbs = throughput.mb_per_s ? `${throughput.mb_per_s.toFixed(1)} MB/s` : '';
                const thr = [fps, mbs].filter(Boolean).join(' ');
                if (thr) extra += ` • ${thr}`;
            }
            stageEl.textContent = stage ? `Stage: ${stage}${extra}` : extra;
        }
        
        // Show/hide cancel button based on operation stage
        if (cancelBtn) {
            const canCancel = stage !== 'complete' && stage !== 'auth' && progress < 90;
            cancelBtn.style.display = canCancel ? 'inline-block' : 'none';
        }
        
        // Update progress bar color based on progress
        if (fill) {
            if (progress >= 100) {
                fill.classList.add('complete');
            } else if (progress >= 75) {
                fill.classList.add('near-complete');
            }
        }
    }
    
    showGlobalActivity(message) {
        const indicator = this.getOrCreateActivityIndicator();
        const text = indicator.querySelector('.activity-text');
        if (text) {
            text.textContent = message;
        }
        indicator.classList.add('show');
    }
    
    hideGlobalActivity() {
        const indicator = document.getElementById('activity-indicator');
        if (indicator) {
            indicator.classList.remove('show');
        }
    }
    
    getOrCreateActivityIndicator() {
        let indicator = document.getElementById('activity-indicator');
        if (!indicator) {
            indicator = document.createElement('div');
            indicator.id = 'activity-indicator';
            indicator.className = 'activity-indicator';
            indicator.innerHTML = `
                <div class="activity-dot"></div>
                <span class="activity-text">Processing...</span>
            `;
            document.body.appendChild(indicator);
        }
        return indicator;
    }
    
    async cancelOperation(operationId) {
        console.log('Cancellation requested for operation:', operationId);
        try {
            const { invoke } = window.__TAURI__.tauri;
            await invoke('cancel_operation', { operation_id: operationId });
            if (window.showNotification) {
                window.showNotification('Operation canceled', 'info');
            }
        } catch (e) {
            console.error('Failed to cancel operation', e);
            if (window.showNotification) {
                window.showNotification('Failed to cancel operation', 'error');
            }
        }
    }
    
    // Public methods for manual progress management
    startManualProgress(operationId, operationType, estimatedDuration) {
        this.onOperationStart({
            operation_id: operationId,
            operation_type: operationType,
            estimated_duration: estimatedDuration
        });
    }
    
    updateManualProgress(operationId, progress, message, stage = '') {
        this.onProgressUpdate({
            operation_id: operationId,
            progress,
            message,
            stage,
            can_cancel: true
        });
    }
    
    completeManualProgress(operationId, success = true, message = 'Operation completed') {
        this.onOperationComplete({
            operation_id: operationId,
            success,
            message,
            duration: 0
        });
    }
}

// Global progress manager instance
let progressManager;

// Initialize when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
        progressManager = new ProgressManager();
        window.progressManager = progressManager;
    });
} else {
    progressManager = new ProgressManager();
    window.progressManager = progressManager;
}
