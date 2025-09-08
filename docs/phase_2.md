# Phase 2: Frontend Optimization - Implementation Plan

## 🎯 **Objective**
Implement real-time progress reporting, loading states, and improved user experience for all long-running operations in the macOS Optimizer application.

## 📋 **Current State Analysis**

### **Existing Infrastructure**
- ✅ All backend commands are async (Phase 1 complete)
- ✅ Basic notifications system exists (`showNotification` function)
- ✅ Animation utilities already available in CSS
- ✅ Modal system exists for confirmations
- ❌ No progress event system
- ❌ No real-time operation feedback
- ❌ No operation cancellation capability
- ❌ No debouncing for rapid clicks

### **Files That Need Modification**
1. **Backend (Rust)**:
   - `src-tauri/src/lib.rs` - Add progress event emission
   - `src-tauri/src/memory_optimizer.rs` - Add progress reporting
   - `src-tauri/src/file_cleaner/engine.rs` - Add scan progress events
   - `src-tauri/src/system_info.rs` - Add progress for system scans

2. **Frontend (JavaScript)**:
   - `src/main.js` - Add event listeners and progress handlers
   - `src/index.html` - Add progress UI components
   - `src/styles.css` - Add progress bar styles

3. **New Files to Create**:
   - `src/js/progress-manager.js` - Centralized progress handling
   - `src/js/operation-queue.js` - Operation queue management
   - `src/styles/components/progress.css` - Progress component styles

---

## 📦 **Implementation Tasks**

### **Task 1: Backend Progress Event System**

#### **1.1 Add Progress Event Types** (`src-tauri/src/lib.rs`)
```rust
#[derive(Clone, serde::Serialize)]
struct ProgressEvent {
    operation: String,
    progress: f32,  // 0.0 to 100.0
    message: String,
    stage: String,
    can_cancel: bool,
}

#[derive(Clone, serde::Serialize)]
struct OperationStartEvent {
    operation_id: String,
    operation_type: String,
    estimated_duration: Option<u32>, // milliseconds
}

#[derive(Clone, serde::Serialize)]
struct OperationCompleteEvent {
    operation_id: String,
    success: bool,
    message: String,
    duration: u32, // actual duration in ms
}
```

#### **1.2 Modify Commands to Emit Progress**
Update each long-running command to emit progress events:

```rust
// Example for memory optimization
#[tauri::command]
async fn optimize_memory(app_handle: tauri::AppHandle, state: State<'_, AppState>) -> Result<MemoryOptimizationResult, String> {
    let operation_id = uuid::Uuid::new_v4().to_string();
    
    // Emit start event
    app_handle.emit("operation:start", OperationStartEvent {
        operation_id: operation_id.clone(),
        operation_type: "memory_optimization".to_string(),
        estimated_duration: Some(3000),
    }).ok();
    
    // Progress updates during operation
    app_handle.emit("progress:update", ProgressEvent {
        operation: operation_id.clone(),
        progress: 25.0,
        message: "Clearing application caches...".to_string(),
        stage: "cache_clear".to_string(),
        can_cancel: true,
    }).ok();
    
    // ... rest of operation
}
```

#### **1.3 Commands Requiring Progress Events**
- [x] `optimize_memory` - 4-5 progress stages
- [x] `optimize_memory_admin` - 6-7 progress stages  
- [x] `scan_cleanable_files` - Progress per directory
- [x] `clean_files` - Progress per file deleted
- [x] `get_processes` - Initial load progress
- [x] `empty_trash` - Progress for large trash

---

### **Task 2: Frontend Progress Manager**

#### **2.1 Create Progress Manager Module** (`src/js/progress-manager.js`)
```javascript
class ProgressManager {
    constructor() {
        this.activeOperations = new Map();
        this.progressBars = new Map();
        this.setupEventListeners();
    }
    
    setupEventListeners() {
        window.__TAURI__.event.listen('operation:start', (event) => {
            this.onOperationStart(event.payload);
        });
        
        window.__TAURI__.event.listen('progress:update', (event) => {
            this.onProgressUpdate(event.payload);
        });
        
        window.__TAURI__.event.listen('operation:complete', (event) => {
            this.onOperationComplete(event.payload);
        });
    }
    
    createProgressBar(operationId, container) {
        const progressBar = document.createElement('div');
        progressBar.className = 'operation-progress';
        progressBar.innerHTML = `
            <div class="progress-header">
                <span class="progress-title"></span>
                <button class="progress-cancel" data-operation="${operationId}">Cancel</button>
            </div>
            <div class="progress-bar-container">
                <div class="progress-bar-fill"></div>
            </div>
            <div class="progress-message"></div>
            <div class="progress-stage"></div>
        `;
        container.appendChild(progressBar);
        this.progressBars.set(operationId, progressBar);
        return progressBar;
    }
    
    updateProgress(operationId, progress, message, stage) {
        const bar = this.progressBars.get(operationId);
        if (!bar) return;
        
        const fill = bar.querySelector('.progress-bar-fill');
        const messageEl = bar.querySelector('.progress-message');
        const stageEl = bar.querySelector('.progress-stage');
        
        fill.style.width = `${progress}%`;
        messageEl.textContent = message;
        stageEl.textContent = `Stage: ${stage}`;
    }
}
```

#### **2.2 Integrate Progress Manager** (`src/main.js`)
```javascript
// Initialize progress manager
const progressManager = new ProgressManager();

// Update existing functions to show progress
async function optimizeMemory() {
    const container = document.getElementById('memory-progress-container');
    container.style.display = 'block';
    
    try {
        const result = await invoke('optimize_memory');
        // Progress will be handled by events
    } catch (error) {
        console.error('Error:', error);
    }
}
```

---

### **Task 3: Loading States & Skeleton Screens**

#### **3.1 Add Skeleton Components** (`src/index.html`)
```html
<!-- Skeleton loader for dashboard stats -->
<div class="skeleton-loader" id="dashboard-skeleton">
    <div class="skeleton-stat-card">
        <div class="skeleton-line skeleton-title"></div>
        <div class="skeleton-line skeleton-value"></div>
        <div class="skeleton-line skeleton-detail"></div>
        <div class="skeleton-progress"></div>
    </div>
</div>

<!-- Loading overlay for operations -->
<div class="loading-overlay" id="global-loading">
    <div class="loading-spinner">
        <svg class="spinner" viewBox="0 0 50 50">
            <circle class="path" cx="25" cy="25" r="20" fill="none" stroke-width="5"></circle>
        </svg>
    </div>
    <div class="loading-text">Processing...</div>
</div>
```

#### **3.2 Add Skeleton Styles** (`src/styles/components/progress.css`)
```css
/* Skeleton Loaders */
.skeleton-loader {
    padding: 20px;
}

.skeleton-line {
    background: linear-gradient(90deg, #f0f0f0 25%, #e0e0e0 50%, #f0f0f0 75%);
    background-size: 200% 100%;
    animation: skeleton 1.5s ease-in-out infinite;
    border-radius: 4px;
    margin-bottom: 10px;
}

.skeleton-title {
    height: 16px;
    width: 60%;
}

.skeleton-value {
    height: 32px;
    width: 40%;
    margin: 10px 0;
}

.skeleton-detail {
    height: 14px;
    width: 80%;
}

.skeleton-progress {
    height: 4px;
    width: 100%;
    background: linear-gradient(90deg, #007AFF 0%, #007AFF 30%, #f0f0f0 30%);
    background-size: 200% 100%;
    animation: skeleton 1.5s ease-in-out infinite;
}

/* Loading Overlay */
.loading-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(10px);
    display: none;
    align-items: center;
    justify-content: center;
    z-index: 9999;
}

.loading-overlay.show {
    display: flex;
}

.spinner {
    animation: rotate 2s linear infinite;
    width: 50px;
    height: 50px;
}

.spinner .path {
    stroke: #007AFF;
    stroke-linecap: round;
    animation: dash 1.5s ease-in-out infinite;
}

@keyframes rotate {
    100% { transform: rotate(360deg); }
}

@keyframes dash {
    0% {
        stroke-dasharray: 1, 150;
        stroke-dashoffset: 0;
    }
    50% {
        stroke-dasharray: 90, 150;
        stroke-dashoffset: -35;
    }
    100% {
        stroke-dasharray: 90, 150;
        stroke-dashoffset: -124;
    }
}
```

---

### **Task 4: Operation Debouncing & Queue**

#### **4.1 Create Operation Queue** (`src/js/operation-queue.js`)
```javascript
class OperationQueue {
    constructor(maxConcurrent = 3) {
        this.queue = [];
        this.running = new Map();
        this.maxConcurrent = maxConcurrent;
        this.debounceTimers = new Map();
    }
    
    async add(operation, options = {}) {
        const { debounce = 0, priority = 0, id = null } = options;
        
        // Debounce if needed
        if (debounce > 0 && id) {
            if (this.debounceTimers.has(id)) {
                clearTimeout(this.debounceTimers.get(id));
            }
            
            return new Promise((resolve) => {
                const timer = setTimeout(() => {
                    this.debounceTimers.delete(id);
                    this.enqueue(operation, priority).then(resolve);
                }, debounce);
                this.debounceTimers.set(id, timer);
            });
        }
        
        return this.enqueue(operation, priority);
    }
    
    async enqueue(operation, priority) {
        return new Promise((resolve, reject) => {
            this.queue.push({ operation, priority, resolve, reject });
            this.queue.sort((a, b) => b.priority - a.priority);
            this.processQueue();
        });
    }
    
    async processQueue() {
        while (this.queue.length > 0 && this.running.size < this.maxConcurrent) {
            const item = this.queue.shift();
            const id = Date.now().toString();
            
            this.running.set(id, item);
            
            try {
                const result = await item.operation();
                item.resolve(result);
            } catch (error) {
                item.reject(error);
            } finally {
                this.running.delete(id);
                this.processQueue();
            }
        }
    }
}

// Global queue instance
const operationQueue = new OperationQueue(3);
```

#### **4.2 Update Button Handlers with Debouncing** (`src/main.js`)
```javascript
// Debounced scan function
const scanForCleanableFiles = async () => {
    return operationQueue.add(
        async () => {
            // Show loading state
            const scanProgress = document.getElementById('scan-progress');
            scanProgress.style.display = 'block';
            
            try {
                const report = await invoke('scan_cleanable_files');
                // Handle results
                return report;
            } finally {
                scanProgress.style.display = 'none';
            }
        },
        { debounce: 1000, id: 'scan-files', priority: 1 }
    );
};

// Prevent rapid clicks on optimize button
document.getElementById('optimize-memory').addEventListener('click', async () => {
    const button = event.target;
    if (button.disabled) return;
    
    button.disabled = true;
    button.classList.add('loading');
    
    try {
        await operationQueue.add(
            () => invoke('optimize_memory'),
            { debounce: 500, id: 'optimize-memory', priority: 2 }
        );
    } finally {
        button.disabled = false;
        button.classList.remove('loading');
    }
});
```

---

### **Task 5: Background Processing Indicators**

#### **5.1 Add Activity Indicator Component** (`src/index.html`)
```html
<!-- Global activity indicator -->
<div class="activity-indicator" id="activity-indicator">
    <div class="activity-dot"></div>
    <span class="activity-text">Background operation in progress</span>
</div>

<!-- Operation history panel -->
<div class="operation-history" id="operation-history">
    <h4>Recent Operations</h4>
    <ul class="history-list">
        <!-- Dynamically populated -->
    </ul>
</div>
```

#### **5.2 Activity Indicator Styles** (`src/styles/components/progress.css`)
```css
/* Activity Indicator */
.activity-indicator {
    position: fixed;
    bottom: 20px;
    right: 20px;
    background: var(--bg-card);
    border-radius: 8px;
    padding: 12px 16px;
    box-shadow: var(--shadow-lg);
    display: none;
    align-items: center;
    gap: 12px;
    z-index: 1000;
}

.activity-indicator.show {
    display: flex;
    animation: slideInRight var(--transition-base);
}

.activity-dot {
    width: 8px;
    height: 8px;
    background: #34C759;
    border-radius: 50%;
    animation: pulseSoft 1.5s ease-in-out infinite;
}

/* Operation History */
.operation-history {
    position: fixed;
    top: 80px;
    right: -300px;
    width: 280px;
    background: var(--bg-card);
    border-radius: 8px;
    padding: 16px;
    box-shadow: var(--shadow-xl);
    transition: right var(--transition-base);
    max-height: 400px;
    overflow-y: auto;
}

.operation-history.show {
    right: 20px;
}

.history-list {
    list-style: none;
    padding: 0;
    margin: 12px 0 0 0;
}

.history-item {
    padding: 8px;
    border-bottom: 1px solid var(--border-color);
    font-size: 12px;
}

.history-item.success {
    border-left: 3px solid #34C759;
}

.history-item.error {
    border-left: 3px solid #FF3B30;
}
```

#### **5.3 Activity Manager** (`src/js/activity-manager.js`)
```javascript
class ActivityManager {
    constructor() {
        this.activeOperations = new Set();
        this.history = [];
        this.maxHistory = 20;
    }
    
    showActivity(message) {
        const indicator = document.getElementById('activity-indicator');
        const text = indicator.querySelector('.activity-text');
        text.textContent = message;
        indicator.classList.add('show');
    }
    
    hideActivity() {
        const indicator = document.getElementById('activity-indicator');
        indicator.classList.remove('show');
    }
    
    addToHistory(operation, success = true, duration = 0) {
        const item = {
            operation,
            success,
            duration,
            timestamp: new Date().toLocaleTimeString()
        };
        
        this.history.unshift(item);
        if (this.history.length > this.maxHistory) {
            this.history.pop();
        }
        
        this.updateHistoryUI();
    }
    
    updateHistoryUI() {
        const list = document.querySelector('.history-list');
        if (!list) return;
        
        list.innerHTML = this.history.map(item => `
            <li class="history-item ${item.success ? 'success' : 'error'}">
                <div class="history-operation">${item.operation}</div>
                <div class="history-meta">
                    <span class="history-time">${item.timestamp}</span>
                    ${item.duration ? `<span class="history-duration">${item.duration}ms</span>` : ''}
                </div>
            </li>
        `).join('');
    }
}
```

---

### **Task 6: Smart UI Updates**

#### **6.1 Progressive Data Loading**
```javascript
// Load dashboard data progressively
async function loadDashboardProgressive() {
    // Show skeleton immediately
    showDashboardSkeleton();
    
    // Load critical data first
    const criticalData = await Promise.race([
        invoke('get_memory_stats'),
        new Promise(resolve => setTimeout(() => resolve(null), 1000))
    ]);
    
    if (criticalData) {
        updateMemoryCard(criticalData);
    }
    
    // Load remaining data
    const [cpuInfo, diskInfo, systemInfo] = await Promise.allSettled([
        invoke('get_cpu_info'),
        invoke('get_disks'),
        invoke('get_system_info')
    ]);
    
    // Update UI progressively
    if (cpuInfo.status === 'fulfilled') updateCpuCard(cpuInfo.value);
    if (diskInfo.status === 'fulfilled') updateDiskCard(diskInfo.value);
    if (systemInfo.status === 'fulfilled') updateSystemCard(systemInfo.value);
    
    hideDashboardSkeleton();
}
```

#### **6.2 Optimistic UI Updates**
```javascript
// Optimistically update UI before server response
async function killProcessOptimistic(pid, name) {
    // Immediately remove from UI
    const row = document.querySelector(`tr[data-pid="${pid}"]`);
    if (row) {
        row.style.opacity = '0.5';
        row.classList.add('removing');
    }
    
    try {
        await invoke('kill_process', { pid });
        if (row) row.remove();
        showNotification(`Process ${name} terminated`, 'success');
    } catch (error) {
        // Revert on error
        if (row) {
            row.style.opacity = '1';
            row.classList.remove('removing');
        }
        showNotification(`Failed to terminate process: ${error}`, 'error');
    }
}
```

---

## 🎯 **Implementation Priority & Timeline**

### **Phase 2.1: Core Progress System (2-3 days)**
1. [ ] Backend progress event types and infrastructure
2. [ ] Modify 3 critical commands to emit progress (memory optimization, file scanning, file cleaning)
3. [ ] Frontend progress event listeners
4. [ ] Basic progress bar UI component

### **Phase 2.2: Loading States (1-2 days)**
1. [ ] Skeleton screens for dashboard
2. [ ] Loading overlays for operations
3. [ ] Activity indicators
4. [ ] Progressive data loading

### **Phase 2.3: Operation Management (2-3 days)**
1. [ ] Operation queue implementation
2. [ ] Debouncing for all action buttons
3. [ ] Operation history panel
4. [ ] Concurrent operation limits

### **Phase 2.4: Polish & Testing (1-2 days)**
1. [ ] Optimistic UI updates
2. [ ] Error state handling
3. [ ] Animation transitions
4. [ ] Cross-operation testing

---

## 📊 **Success Metrics**

### **Technical Metrics**
- [ ] All operations show real-time progress (0-100%)
- [ ] No button can be clicked twice within 500ms
- [ ] Maximum 3 concurrent operations
- [ ] Progress updates at least every 500ms

### **User Experience Metrics**
- [ ] User always knows what's happening (no silent waits)
- [ ] Every operation can show estimated time
- [ ] Failed operations show clear error messages
- [ ] Success operations show completion confirmation

### **Performance Metrics**
- [ ] Progress events don't block main thread
- [ ] UI remains responsive during all operations
- [ ] Animation frame rate stays above 30fps
- [ ] Memory usage for progress tracking < 5MB

---

## 🔧 **Testing Checklist**

### **Unit Tests**
- [ ] Progress event emission from backend
- [ ] Operation queue with priority
- [ ] Debounce functionality
- [ ] History management with max items

### **Integration Tests**
- [ ] Progress updates during memory optimization
- [ ] File scan with real-time feedback
- [ ] Concurrent operation handling
- [ ] Error recovery and UI state

### **User Experience Tests**
- [ ] Rapid button clicking prevention
- [ ] Progress bar accuracy
- [ ] Loading state transitions
- [ ] History panel updates

---

## 📝 **Notes for Implementation**

### **Important Considerations**
1. **Event Names**: Use consistent naming: `operation:start`, `progress:update`, `operation:complete`
2. **Operation IDs**: Use UUIDs to track operations uniquely
3. **Memory Management**: Clean up completed operations from maps
4. **Error Handling**: Always emit `operation:complete` even on error
5. **Accessibility**: Add ARIA labels to progress bars

### **Potential Challenges**
1. **Cross-platform Testing**: Test on different macOS versions
2. **WebView Compatibility**: Ensure CSS animations work in Tauri WebView
3. **Event Ordering**: Handle out-of-order progress events
4. **Resource Usage**: Monitor memory usage with many operations

### **Future Enhancements**
1. **Cancellation**: Add backend support for operation cancellation
2. **Persistence**: Save operation history to disk
3. **Analytics**: Track operation performance metrics
4. **Notifications**: System notifications for background operations

---

## 🚀 **Getting Started**

### **For the Build Agent**
1. Start with Task 1.1 - Create progress event types in Rust
2. Implement one command fully (recommend `optimize_memory`) as reference
3. Create the ProgressManager class in JavaScript
4. Test with a single operation end-to-end
5. Then parallelize remaining implementation

### **Key Commands**
```bash
# Build and test
npm run tauri dev

# Check for TypeScript/JavaScript errors
npm run lint

# Build for production
npm run tauri build
```

---

*This plan provides a complete roadmap for implementing Phase 2 of the macOS Optimizer performance improvements, focusing on user experience and real-time feedback.*