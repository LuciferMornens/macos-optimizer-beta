const { invoke } = window.__TAURI__.core;

// Unified user confirmation helper that prefers Tauri's dialog
async function userConfirm(message, { title = 'Confirm', kind = 'warning' } = {}) {
    try {
        const tauri = window.__TAURI__;
        if (tauri && tauri.dialog) {
            if (typeof tauri.dialog.ask === 'function') {
                const result = await tauri.dialog.ask(message, { title, kind });
                return !!result;
            }
            // Avoid tauri.dialog.confirm in Tauri v2 for boolean decisions (may resolve void)
        }
    } catch (e) {
        console.warn('Tauri dialog unavailable, falling back to window.confirm:', e);
    }
    // Fallback to an inline modal instead of window.confirm (WebView can block it)
    return await inlineConfirm(message, { title, kind });
}

function inlineConfirm(message, { title = 'Confirm', kind = 'warning' } = {}) {
    return new Promise(resolve => {
        // Avoid multiple overlays
        if (document.getElementById('inline-confirm-overlay')) {
            resolve(false);
            return;
        }

        const overlay = document.createElement('div');
        overlay.id = 'inline-confirm-overlay';
        overlay.className = 'modal-overlay';

        const card = document.createElement('div');
        card.className = `modal-card ${kind === 'warning' ? 'modal-danger' : ''}`;

        const header = document.createElement('div');
        header.className = 'modal-header';
        
        const heading = document.createElement('h4');
        heading.className = 'modal-title';
        heading.textContent = title || 'Confirm';
        
        header.appendChild(heading);

        const body = document.createElement('div');
        body.className = 'modal-body';
        body.textContent = message;

        const actions = document.createElement('div');
        actions.className = 'modal-actions';

        const cancelBtn = document.createElement('button');
        cancelBtn.textContent = 'Cancel';
        cancelBtn.className = 'btn btn-secondary';

        const okBtn = document.createElement('button');
        okBtn.textContent = kind === 'warning' ? 'Confirm' : 'OK';
        okBtn.className = kind === 'warning' ? 'btn btn-danger' : 'btn btn-primary';

        const cleanup = () => {
            document.removeEventListener('keydown', onKey);
            overlay.remove();
        };

        const onCancel = () => { cleanup(); resolve(false); };
        const onOk = () => { cleanup(); resolve(true); };

        const onKey = (e) => {
            if (e.key === 'Escape') onCancel();
            if (e.key === 'Enter') onOk();
        };

        cancelBtn.addEventListener('click', onCancel);
        okBtn.addEventListener('click', onOk);
        document.addEventListener('keydown', onKey);

        actions.appendChild(cancelBtn);
        actions.appendChild(okBtn);
        card.appendChild(header);
        card.appendChild(body);
        card.appendChild(actions);
        overlay.appendChild(card);
        document.body.appendChild(overlay);

        // Focus OK for quick keyboard confirm
        setTimeout(() => okBtn.focus(), 0);
    });
}

// Utility functions
function formatBytes(bytes) {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

function formatUptime(seconds) {
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    
    if (days > 0) {
        return `${days}d ${hours}h ${minutes}m`;
    } else if (hours > 0) {
        return `${hours}h ${minutes}m`;
    } else {
        return `${minutes}m`;
    }
}

function showNotification(message, type = 'success') {
    const notification = document.getElementById('notification');
    const messageElement = notification.querySelector('.notification-message');
    
    messageElement.textContent = message;
    notification.className = `notification show ${type}`;
    
    setTimeout(() => {
        notification.classList.remove('show');
    }, 3000);
}

// Tab navigation - will be set up in DOMContentLoaded
function setupTabNavigation() {
    document.querySelectorAll('.nav-item').forEach(item => {
        item.addEventListener('click', () => {
            const tabName = item.dataset.tab;
            
            // Update nav items
            document.querySelectorAll('.nav-item').forEach(nav => {
                nav.classList.remove('active');
            });
            item.classList.add('active');
            
            // Update content
            document.querySelectorAll('.tab-content').forEach(content => {
                content.classList.remove('active');
            });
            document.getElementById(tabName).classList.add('active');
            
            // Always stop processes auto-refresh when switching tabs
            stopProcessesAutoRefresh();

            // Load tab-specific data
            switch(tabName) {
                case 'dashboard':
                    loadDashboard();
                    break;
                case 'memory':
                    loadMemoryInfo();
                    // Debug: Check button visibility when Memory tab is shown
                    setTimeout(() => {
                        const deepCleanBtn = document.getElementById('deep-clean-memory');
                        if (deepCleanBtn) {
                            console.log('Memory tab active - Deep clean button visible:', deepCleanBtn.offsetParent !== null);
                        }
                    }, 100);
                    break;
                case 'storage':
                    // Storage tab doesn't need auto-load, user will scan manually
                    break;
                case 'processes':
                    loadProcesses();
                    startProcessesAutoRefresh();
                    break;
            }
        });
    });
}

// Dashboard functions
async function loadDashboard() {
    console.log('Loading dashboard...');
    try {
        // Load system info
        const systemInfo = await invoke('get_system_info');
        console.log('System info:', systemInfo);
        const memStats = await invoke('get_memory_stats');
        console.log('Memory stats:', memStats);
        const cpuInfo = await invoke('get_cpu_info');
        console.log('CPU info:', cpuInfo);
        const disks = await invoke('get_disks');
        console.log('Disks:', disks);
        
        // Update memory stats (unified source)
        const memoryPercent = (memStats.used / memStats.total * 100).toFixed(1);
        document.getElementById('memory-usage').textContent = `${memoryPercent}%`;
        document.getElementById('memory-detail').textContent = 
            `${formatBytes(memStats.used)} / ${formatBytes(memStats.total)}`;
        document.getElementById('memory-progress').style.width = `${memoryPercent}%`;
        
        // Update CPU stats
        document.getElementById('cpu-usage').textContent = `${cpuInfo.cpu_usage.toFixed(1)}%`;
        document.getElementById('cpu-detail').textContent = `${cpuInfo.core_count} cores`;
        document.getElementById('cpu-progress').style.width = `${cpuInfo.cpu_usage}%`;
        
        // Update disk stats (use first disk)
        if (disks.length > 0) {
            const mainDisk = disks[0];
            const diskPercent = (mainDisk.used_space / mainDisk.total_space * 100).toFixed(1);
            document.getElementById('disk-usage').textContent = `${diskPercent}%`;
            document.getElementById('disk-detail').textContent = 
                `${formatBytes(mainDisk.used_space)} / ${formatBytes(mainDisk.total_space)}`;
            document.getElementById('disk-progress').style.width = `${diskPercent}%`;
        }
        
        // Update system info
        document.getElementById('uptime').textContent = formatUptime(systemInfo.uptime);
        document.getElementById('system-info').textContent = systemInfo.os_name;
        document.getElementById('os-version').textContent = systemInfo.os_version;
        document.getElementById('hostname').textContent = systemInfo.hostname;
        document.getElementById('boot-time').textContent = 
            new Date(systemInfo.boot_time * 1000).toLocaleString();
        
    } catch (error) {
        console.error('Error loading dashboard:', error);
        showNotification('Failed to load dashboard data', 'error');
    }
}

// Memory management functions
async function loadMemoryInfo() {
    try {
        // Use unified memory stats for accuracy across UI and results
        const stats = await invoke('get_memory_stats');
        const usedRatio = stats.total > 0 ? (stats.used / stats.total) : 0;
        
        document.getElementById('total-memory').textContent = formatBytes(stats.total);
        document.getElementById('used-memory').textContent = formatBytes(stats.used);
        document.getElementById('available-memory').textContent = formatBytes(stats.available);
        document.getElementById('memory-pressure').textContent = `${(usedRatio * 100).toFixed(1)}%`;
        document.getElementById('swap-used').textContent = formatBytes(stats.swap_used);
        
        // Draw memory chart (simple visual representation)
        const canvas = document.getElementById('memory-chart');
        if (canvas) {
            const ctx = canvas.getContext('2d');
            const width = canvas.width = canvas.offsetWidth;
            const height = canvas.height = canvas.offsetHeight;
            
            // Clear canvas
            ctx.clearRect(0, 0, width, height);
            
            // Draw pie chart
            const centerX = width / 2;
            const centerY = height / 2;
            const radius = Math.min(width, height) / 3;
            
            const usedAngle = usedRatio * Math.PI * 2;
            
            // Used memory
            ctx.beginPath();
            ctx.arc(centerX, centerY, radius, 0, usedAngle);
            ctx.lineTo(centerX, centerY);
            ctx.fillStyle = '#007AFF';
            ctx.fill();
            
            // Available memory
            ctx.beginPath();
            ctx.arc(centerX, centerY, radius, usedAngle, Math.PI * 2);
            ctx.lineTo(centerX, centerY);
            ctx.fillStyle = '#34C759';
            ctx.fill();
            
            // Center text
            ctx.fillStyle = '#ffffff';
            ctx.font = 'bold 24px -apple-system';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText(`${(usedRatio * 100).toFixed(0)}%`, centerX, centerY);
        }
    } catch (error) {
        console.error('Error loading memory info:', error);
        showNotification('Failed to load memory information', 'error');
    }
}

// Storage cleaner functions
let cleanableFiles = [];
let lastReport = null;
let showAdvanced = false;
let currentCategoryFilter = null;

async function scanForCleanableFiles() {
    const scanProgress = document.getElementById('scan-progress');
    const cleaningReport = document.getElementById('cleaning-report');
    const cleanButton = document.getElementById('clean-selected');
    
    scanProgress.style.display = 'block';
    cleaningReport.style.display = 'none';
    cleanButton.disabled = true;
    
    try {
        const report = await invoke('scan_cleanable_files');
        lastReport = report;
        cleanableFiles = await invoke('get_cleanable_files');
        
        // Update summary
        document.getElementById('total-cleanable').textContent = formatBytes(report.total_size);
        document.getElementById('files-found').textContent = report.files_count;
        
        // Display categories
        renderCategories();
        
        // Display files
        currentCategoryFilter = null;
        displayFiles(cleanableFiles);
        
        scanProgress.style.display = 'none';
        cleaningReport.style.display = 'block';
        cleanButton.disabled = false;
        
        showNotification(`Found ${report.files_count} cleanable files (${formatBytes(report.total_size)})`, 'success');
    } catch (error) {
        console.error('Error scanning files:', error);
        showNotification('Failed to scan for cleanable files', 'error');
        scanProgress.style.display = 'none';
    }
}

function renderCategories() {
    if (!lastReport) return;
    const categoriesList = document.getElementById('categories-list');
    categoriesList.innerHTML = '';
    const advancedSet = new Set((lastReport.advanced_categories || []).map(x => x.toLowerCase()));

    // Build a lookup for existing categories
    const present = new Map();
    lastReport.categories.forEach(c => present.set(c.name.toLowerCase(), c));

    // First render non-advanced (always), then advanced (only if toggled)
    lastReport.categories.forEach(category => {
        const isAdvanced = advancedSet.has(category.name.toLowerCase()) || /\(advanced\)/i.test(category.name);
        if (!isAdvanced) {
            const card = createCategoryCard(category.name, category.size, category.count, false);
            categoriesList.appendChild(card);
        }
    });

    if (showAdvanced) {
        // Render advanced that have items from the report
        lastReport.categories.forEach(category => {
            const isAdvanced = advancedSet.has(category.name.toLowerCase()) || /\(advanced\)/i.test(category.name);
            if (isAdvanced) {
                const card = createCategoryCard(category.name, category.size, category.count, true);
                categoriesList.appendChild(card);
            }
        });

        // Also render advanced categories that exist in rules but had zero results
        (lastReport.advanced_categories || []).forEach(name => {
            if (!present.has(name.toLowerCase())) {
                const card = createCategoryCard(name, 0, 0, true);
                categoriesList.appendChild(card);
            }
        });
    }
}

function createCategoryCard(name, size, count, isAdvanced = false) {
    const categoryCard = document.createElement('div');
    categoryCard.className = 'category-card';
    categoryCard.dataset.name = name;
    const badge = isAdvanced ? '<span class="badge badge-advanced">Advanced</span>' : '';
    categoryCard.innerHTML = `
        <div class="category-name">${name} ${badge}</div>
        <div class="category-size">${formatBytes(size)} (${count} files)</div>
    `;
    categoryCard.addEventListener('click', () => filterFilesByCategory(name));
    return categoryCard;
}

function displayFiles(files) {
    const filesList = document.getElementById('files-list');
    filesList.innerHTML = '';
    
    // Sort files by safety score (highest first) and size (largest first)
    const sortedFiles = [...files].sort((a, b) => {
        if (b.safety_score !== a.safety_score) {
            return b.safety_score - a.safety_score;
        }
        return b.size - a.size;
    });
    
    if (sortedFiles.length === 0) {
        const empty = document.createElement('div');
        empty.className = 'empty-state';
        empty.textContent = currentCategoryFilter ? 'No files in this category' : 'No files found';
        filesList.appendChild(empty);
        updateSelectionInfo();
        return;
    }

    sortedFiles.slice(0, 100).forEach((file, index) => {
        const fileItem = document.createElement('div');
        fileItem.className = 'file-item';
        
        // Determine safety badge color
        let safetyClass = 'safety-low';
        let safetyText = 'Low';
        if (file.safety_score >= 95) {
            safetyClass = 'safety-very-high';
            safetyText = 'Very Safe';
        } else if (file.safety_score >= 80) {
            safetyClass = 'safety-high';
            safetyText = 'Safe';
        } else if (file.safety_score >= 60) {
            safetyClass = 'safety-medium';
            safetyText = 'Review';
        }
        
        fileItem.innerHTML = `
            <input type="checkbox" id="file-${index}" value="${file.path}" 
                   data-size="${file.size}"
                   ${file.safe_to_delete ? '' : 'disabled'}
                   ${file.auto_select ? 'checked' : ''}>
            <div class="file-info">
                <div class="file-header">
                    <span class="file-path">${file.path}</span>
                    <span class="safety-badge ${safetyClass}" title="Safety Score: ${file.safety_score}/100">
                        ${safetyText} (${file.safety_score})
                    </span>
                </div>
                <div class="file-details">
                    <span class="file-size">${formatBytes(file.size)}</span>
                    <span class="file-category">${file.category}</span>
                    <span class="file-description">${file.description}</span>
                </div>
            </div>
        `;
        filesList.appendChild(fileItem);
    });
    
    // Update selection count after displaying files
    updateSelectionInfo();
}

function filterFilesByCategory(categoryName) {
    currentCategoryFilter = categoryName;
    const filtered = cleanableFiles.filter(file => file.category === categoryName);
    displayFiles(filtered);
    
    // Update category selection
    document.querySelectorAll('.category-card').forEach(card => {
        if ((card.dataset && card.dataset.name) === categoryName) {
            card.classList.add('selected');
        } else {
            card.classList.remove('selected');
        }
    });

    // Show category action bar
    const actions = document.getElementById('category-actions');
    const current = document.getElementById('current-category-name');
    if (actions && current) {
        current.textContent = categoryName;
        actions.style.display = 'flex';
    }
}

async function cleanCategory(categoryName) {
    const files = cleanableFiles.filter(f => f.category === categoryName && f.safe_to_delete);
    if (files.length === 0) {
        showNotification('No safe files found in this category', 'warning');
        return;
    }
    const totalSize = files.reduce((acc, f) => acc + (f.size || 0), 0);
    const confirmed = await userConfirm(
        `Clean ${files.length} files in "${categoryName}"?\n\nThis will free approximately ${formatBytes(totalSize)}`,
        { title: 'Clean Category', kind: 'warning' }
    );
    if (!confirmed) return;

    try {
        const [freedBytes, filesDeleted] = await invoke('clean_files', { filePaths: files.map(f => f.path) });
        showNotification(`Cleaned ${filesDeleted} files, freed ${formatBytes(freedBytes)}`, 'success');
        await scanForCleanableFiles();
        // Reset category filter
        const actions = document.getElementById('category-actions');
        if (actions) actions.style.display = 'none';
        currentCategoryFilter = null;
    } catch (error) {
        showNotification(`Failed to clean category: ${error}`, 'error');
    }
}

async function cleanSelectedFiles() {
    console.log('cleanSelectedFiles function called');
    
    const selectedFiles = [];
    let totalSize = 0;
    const checkboxes = document.querySelectorAll('#files-list input[type="checkbox"]:checked');
    
    console.log(`Found ${checkboxes.length} selected checkboxes`);
    
    checkboxes.forEach(checkbox => {
        selectedFiles.push(checkbox.value);
        totalSize += parseInt(checkbox.dataset.size || 0);
    });
    
    console.log(`Selected files: ${selectedFiles.length}, Total size: ${totalSize}`);
    
    if (selectedFiles.length === 0) {
        showNotification('No files selected for cleaning', 'error');
        return;
    }
    
    const confirmed = await userConfirm(
        `Are you sure you want to delete ${selectedFiles.length} files?\n\nThis will free approximately ${formatBytes(totalSize)}`,
        { title: 'Confirm Clean', kind: 'warning' }
    );
    if (!confirmed) {
        console.log('User cancelled file cleaning');
        return;
    }
    
    console.log('User confirmed, starting file cleaning...');
    
    try {
        showNotification('Cleaning selected files...', 'success');
        const [freedBytes, filesDeleted] = await invoke('clean_files', { filePaths: selectedFiles });
        console.log(`Clean complete: ${filesDeleted} files deleted, ${freedBytes} bytes freed`);
        showNotification(`Cleaned ${filesDeleted} files, freed ${formatBytes(freedBytes)}`, 'success');
        
        // Rescan after cleaning
        await scanForCleanableFiles();
    } catch (error) {
        console.error('Error cleaning files:', error);
        showNotification(`Failed to clean selected files: ${error}`, 'error');
    }
}

function updateSelectionInfo() {
    const checkboxes = document.querySelectorAll('#files-list input[type="checkbox"]:checked');
    let totalSize = 0;
    let count = 0;
    
    checkboxes.forEach(checkbox => {
        count++;
        totalSize += parseInt(checkbox.dataset.size || 0);
    });
    
    document.getElementById('selected-count').textContent = `${count} files selected`;
    document.getElementById('selected-size').textContent = formatBytes(totalSize);
}

async function autoSelectSafeFiles() {
    try {
        const autoSelectFiles = await invoke('get_auto_selectable_files');
        
        // Clear current selection
        document.querySelectorAll('#files-list input[type="checkbox"]').forEach(checkbox => {
            checkbox.checked = false;
        });
        
        // Select auto-selectable files
        let selectedCount = 0;
        autoSelectFiles.forEach(file => {
            const checkbox = document.querySelector(`#files-list input[value="${CSS.escape(file.path)}"]`);
            if (checkbox && !checkbox.disabled) {
                checkbox.checked = true;
                selectedCount++;
            }
        });
        
        updateSelectionInfo();
        showNotification(`Auto-selected ${selectedCount} safe files`, 'success');
    } catch (error) {
        console.error('Error auto-selecting files:', error);
        showNotification('Failed to auto-select files', 'error');
    }
}

async function selectBySafety(minScore = 95) {
    try {
        const safeFiles = await invoke('get_files_by_safety', { minSafetyScore: minScore });
        
        // Clear current selection
        document.querySelectorAll('#files-list input[type="checkbox"]').forEach(checkbox => {
            checkbox.checked = false;
        });
        
        // Select files by safety score
        let selectedCount = 0;
        safeFiles.forEach(file => {
            const checkbox = document.querySelector(`#files-list input[value="${CSS.escape(file.path)}"]`);
            if (checkbox && !checkbox.disabled) {
                checkbox.checked = true;
                selectedCount++;
            }
        });
        
        updateSelectionInfo();
        showNotification(`Selected ${selectedCount} files with safety score ≥ ${minScore}`, 'success');
    } catch (error) {
        console.error('Error selecting files by safety:', error);
        showNotification('Failed to select files by safety', 'error');
    }
}

function clearFileSelection() {
    document.querySelectorAll('#files-list input[type="checkbox"]').forEach(checkbox => {
        checkbox.checked = false;
    });
    updateSelectionInfo();
    showNotification('Selection cleared', 'success');
}

// Process management functions
let allProcesses = [];
let processSearchTerm = '';
let processSortBy = 'memory';
let processesAutoRefreshTimer = null;

async function loadProcesses() {
    try {
        allProcesses = await invoke('get_processes');
        renderProcesses();
    } catch (error) {
        console.error('Error loading processes:', error);
        showNotification('Failed to load processes', 'error');
    }
}

function renderProcesses() {
const processList = document.getElementById('processes-list');
processList.innerHTML = '';

// Filter
let view = allProcesses;
if (processSearchTerm && processSearchTerm.length > 0) {
    const term = processSearchTerm;
    view = view.filter(p => p.name.toLowerCase().includes(term));
}

// Sort
switch (processSortBy) {
case 'cpu':
view = [...view].sort((a, b) => b.cpu_usage - a.cpu_usage);
break;
case 'name':
view = [...view].sort((a, b) => a.name.localeCompare(b.name));
break;
case 'memory':
default:
    view = [...view].sort((a, b) => b.memory_usage - a.memory_usage);
        break;
}

// Display top 50
const rows = view.slice(0, 50);

if (rows.length === 0) {
const row = document.createElement('tr');
const cell = document.createElement('td');
cell.colSpan = 5;
cell.textContent = 'No matching processes';
row.appendChild(cell);
processList.appendChild(row);
return;
}

rows.forEach(process => {
const row = document.createElement('tr');
    row.innerHTML = `
            <td>${process.name}</td>
            <td>${process.pid}</td>
            <td>${process.cpu_usage.toFixed(1)}%</td>
            <td>${formatBytes(process.memory_usage)}</td>
            <td>
                <button class="kill-process" data-pid="${process.pid}" data-name="${process.name}">
                    End Task
                </button>
            </td>
        `;
        processList.appendChild(row);
    });

    // Add kill process handlers
    document.querySelectorAll('.kill-process').forEach(button => {
        button.addEventListener('click', async (e) => {
            const btn = e.currentTarget;
            const pid = parseInt(btn.dataset.pid);
            const name = btn.dataset.name;

            if (await userConfirm(`Are you sure you want to end the process "${name}" (PID: ${pid})?`, { title: 'Confirm End Task', kind: 'warning' })) {
                try {
                    await invoke('kill_process', { pid });
                    showNotification(`Process ${name} terminated`, 'success');
                    await loadProcesses();
                } catch (error) {
                    showNotification(`Failed to terminate process: ${error}`, 'error');
                }
            }
        });
    });
}

function startProcessesAutoRefresh() {
    stopProcessesAutoRefresh();
    processesAutoRefreshTimer = setInterval(() => {
        loadProcesses();
    }, 2000);
}

function stopProcessesAutoRefresh() {
    if (processesAutoRefreshTimer) {
        clearInterval(processesAutoRefreshTimer);
        processesAutoRefreshTimer = null;
    }
}

// Search and sort processes
document.getElementById('process-search').addEventListener('input', (e) => {
    processSearchTerm = e.target.value.toLowerCase();
    renderProcesses();
});

document.getElementById('process-sort').addEventListener('change', (e) => {
    processSortBy = e.target.value;
    renderProcesses();
});

// Event listeners
document.getElementById('quick-optimize').addEventListener('click', async () => {
    try {
        showNotification('Starting quick optimization...', 'success');
        
        // Use safe mode for quick optimize (no admin prompt)
        const result = await invoke('optimize_memory');
        
        let message = 'Quick optimization complete!';
        if (result.freed_memory > 0) {
            message += ` Freed ${formatBytes(Math.abs(result.freed_memory))} of memory`;
        }
        
        // Show what was done
        if (result.optimizations_performed && result.optimizations_performed.length > 0) {
            console.log('Optimizations performed:', result.optimizations_performed);
        }
        
        showNotification(message, 'success');
        
        // Reload dashboard
        await loadDashboard();
    } catch (error) {
        console.error('Error optimizing:', error);
        showNotification('Quick optimization completed with limited access. Use Memory tab for full optimization.', 'warning');
    }
});

// Old event handlers removed - now using event delegation in DOMContentLoaded

function setupEventListeners() {
    // Direct listener for deep clean button as fallback
    const deepCleanBtn = document.getElementById('deep-clean-memory');
    if (deepCleanBtn) {
        deepCleanBtn.addEventListener('click', async (e) => {
            e.preventDefault();
            e.stopPropagation(); // Prevent the delegated handler from also firing
            console.log('Deep clean direct listener triggered!');
            
            // Skip confirmation dialog for now - it's not working properly in WebView
            console.log('Deep clean requested, proceeding...');
            
            // You can uncomment this to re-enable confirmation later
            // const confirmed = confirm(
            //     '⚠️ Deep Clean with Administrator Access\n\n' +
            //     'This will:\n' +
            //     '• Purge all disk caches\n' +
            //     '• Clear DNS and network caches\n' +
            //     '• Optimize memory compression\n' +
            //     '• Free inactive memory\n' +
            //     '• Clear application caches\n\n' +
            //     'You will be prompted for your administrator password.\n' +
            //     'Continue?'
            // );
            // 
            // if (!confirmed) {
            //     console.log('User cancelled deep clean');
            //     return;
            // }
            
            console.log('Starting deep clean with admin access...');
            
            try {
                showNotification('Starting deep clean with admin access...', 'success');
                console.log('Invoking optimize_memory_admin...');
                const result = await invoke('optimize_memory_admin');
                console.log('Deep clean result received:', result);
                
                const resultDiv = document.getElementById('optimization-result');
                const resultContent = resultDiv.querySelector('.result-content');
                
                let optimizationsList = '';
                if (result.optimizations_performed && result.optimizations_performed.length > 0) {
                    optimizationsList = '<ul style="color: #34C759;">' +
                        result.optimizations_performed.map(opt => `<li>✓ ${opt}</li>`).join('') +
                        '</ul>';
                }
                
                resultContent.innerHTML = `
                    <h4 style="color: #34C759;">✨ Deep Clean Complete!</h4>
                    <p><strong>Memory Freed:</strong> <span style="color: #34C759; font-size: 24px;">${formatBytes(result.freed_memory)}</span></p>
                    <p><strong>Memory Usage:</strong> ${formatBytes(result.memory_after.used)} / ${formatBytes(result.memory_after.total)}</p>
                    <p><strong>Available Now:</strong> ${formatBytes(result.memory_after.available)}</p>
                    <div style="margin-top: 15px;">
                        <strong>Optimizations Performed:</strong>
                        ${optimizationsList}
                    </div>
                `;
                
                resultDiv.style.display = 'block';
                resultDiv.style.border = '2px solid #34C759';
                
                showNotification(`Deep clean complete! Freed ${formatBytes(result.freed_memory)} of memory`, 'success');
                
                await loadMemoryInfo();
            } catch (error) {
                console.error('Deep clean error:', error);
                showNotification('Deep clean failed: ' + error, 'error');
            }
        });
    }
    
    // Setup other event listeners (non-memory buttons that aren't using delegation)
    const refreshMemoryBtn = document.getElementById('refresh-memory');
    if (refreshMemoryBtn) {
        refreshMemoryBtn.addEventListener('click', loadMemoryInfo);
    }
    
    const refreshProcessesBtn = document.getElementById('refresh-processes');
    if (refreshProcessesBtn) {
        refreshProcessesBtn.addEventListener('click', loadProcesses);
    }
    
    const scanFilesBtn = document.getElementById('scan-files');
    if (scanFilesBtn) {
        scanFilesBtn.addEventListener('click', scanForCleanableFiles);
    }
    
    const emptyTrashBtn = document.getElementById('empty-trash');
    if (emptyTrashBtn) {
        emptyTrashBtn.addEventListener('click', async () => {
            const confirmed = await userConfirm('Empty the Trash now?\n\nThis permanently deletes all items in your Trash.', { title: 'Empty Trash', kind: 'warning' });
            if (!confirmed) return;
            try {
                const [freed, count] = await invoke('empty_trash');
                showNotification(`Emptied Trash: removed ${count} items, freed ${formatBytes(freed)}`, 'success');
                // Refresh scan if results are visible
                const cleaningReport = document.getElementById('cleaning-report');
                if (cleaningReport && cleaningReport.style.display !== 'none') {
                    await scanForCleanableFiles();
                }
            } catch (e) {
                console.error('Empty trash failed:', e);
                showNotification('Failed to empty Trash', 'error');
            }
        });
    }
    
    const toggleAdvanced = document.getElementById('toggle-advanced');
    if (toggleAdvanced) {
        // Initialize from localStorage
        const stored = localStorage.getItem('showAdvanced');
        showAdvanced = stored === 'true';
        toggleAdvanced.checked = showAdvanced;

        toggleAdvanced.addEventListener('change', (e) => {
            showAdvanced = !!e.target.checked;
            localStorage.setItem('showAdvanced', showAdvanced ? 'true' : 'false');
            renderCategories();
            // If current filter is advanced and we hid it, clear filter
            if (!showAdvanced && currentCategoryFilter) {
                const advSet = new Set(((lastReport && lastReport.advanced_categories) || []).map(x => x.toLowerCase()));
                if (advSet.has(currentCategoryFilter.toLowerCase()) || /(advanced)/i.test(currentCategoryFilter)) {
                    currentCategoryFilter = null;
                    displayFiles(cleanableFiles);
                    const actions = document.getElementById('category-actions');
                    if (actions) actions.style.display = 'none';
                }
            }
        });
    }
    
    const cleanSelectedBtn = document.getElementById('clean-selected');
    if (cleanSelectedBtn) {
        console.log('Setting up clean-selected button listener');
        // Mark listener attached so the delayed fallback doesn't double-bind
        cleanSelectedBtn.setAttribute('data-listener-attached', 'true');
        cleanSelectedBtn.addEventListener('click', (e) => {
            console.log('Clean selected button clicked via direct listener');
            e.preventDefault();
            e.stopPropagation();
            cleanSelectedFiles();
        });
    } else {
        console.error('Clean selected button not found!')
    }
    
    // Smart selection buttons
    const autoSelectBtn = document.getElementById('auto-select-safe');
    if (autoSelectBtn) {
        autoSelectBtn.addEventListener('click', autoSelectSafeFiles);
    }
    
    const selectBySafetyBtn = document.getElementById('select-by-safety');
    if (selectBySafetyBtn) {
        selectBySafetyBtn.addEventListener('click', () => selectBySafety(95));
    }
    
    const clearSelectionBtn = document.getElementById('clear-selection');
    if (clearSelectionBtn) {
        clearSelectionBtn.addEventListener('click', clearFileSelection);
    }

    // Category actions
    const clearCategoryBtn = document.getElementById('clear-category-filter');
    if (clearCategoryBtn) {
        clearCategoryBtn.addEventListener('click', () => {
            currentCategoryFilter = null;
            displayFiles(cleanableFiles);
            const actions = document.getElementById('category-actions');
            if (actions) actions.style.display = 'none';
            document.querySelectorAll('.category-card').forEach(c => c.classList.remove('selected'));
        });
    }
    const cleanThisCategoryBtn = document.getElementById('clean-this-category');
    if (cleanThisCategoryBtn) {
        cleanThisCategoryBtn.addEventListener('click', async () => {
            if (currentCategoryFilter) {
                await cleanCategory(currentCategoryFilter);
            }
        });
    }
    
    // Add change listener for file checkboxes using event delegation
    document.addEventListener('change', (event) => {
        if (event.target.matches('#files-list input[type="checkbox"]')) {
            updateSelectionInfo();
        }
    });
    
    // Notification close button
    const notificationClose = document.querySelector('.notification-close');
    if (notificationClose) {
        notificationClose.addEventListener('click', () => {
            document.getElementById('notification').classList.remove('show');
        });
    }
}

// Wait for window to load and Tauri to be ready
window.addEventListener('DOMContentLoaded', () => {
    console.log('DOM loaded, checking Tauri...');
    
    // Setup tab navigation
    setupTabNavigation();
    console.log('Tab navigation setup complete');
    
    // Use event delegation ONLY for optimize-memory button
    document.body.addEventListener('click', async (event) => {
        // Guard against non-Element targets (text nodes in WebKit)
        const tgt = event.target;
        if (!(tgt instanceof Element)) return;
        
        // Check if clicked element is a button or inside a button
        const button = tgt.closest('button');
        if (!button) return;
        
        // Debug log for all buttons
        console.log('Button clicked with ID:', button.id);
        
        // ONLY intercept and handle optimize-memory button
        // All other buttons should work with their direct listeners
        if (button.id === 'optimize-memory') {
            console.log('Optimize memory button handler triggered!');
            event.preventDefault();
            event.stopPropagation();
            
            const useAdmin = confirm(
                'Memory Optimization Options:\n\n' +
                'OK = Deep Optimization (requires admin password)\n' +
                'Cancel = Safe Mode (no admin required)\n\n' +
                'Deep optimization can free more memory but requires administrator access.'
            );
            
            try {
                showNotification('Starting memory optimization...', 'success');
                
                const result = useAdmin
                    ? await invoke('optimize_memory_admin')
                    : await invoke('optimize_memory');
                
                console.log('Optimization result:', result);
                
                const resultDiv = document.getElementById('optimization-result');
                const resultContent = resultDiv.querySelector('.result-content');
                
                let optimizationsList = '';
                if (result.optimizations_performed && result.optimizations_performed.length > 0) {
                    optimizationsList = '<ul>' +
                        result.optimizations_performed.map(opt => `<li>✓ ${opt}</li>`).join('') +
                        '</ul>';
                }
                
                resultContent.innerHTML = `
                    <p><strong>Optimization Type:</strong> ${result.optimization_type}</p>
                    <p><strong>Memory Before:</strong> ${formatBytes(result.memory_before.used)} / ${formatBytes(result.memory_before.total)}</p>
                    <p><strong>Memory After:</strong> ${formatBytes(result.memory_after.used)} / ${formatBytes(result.memory_after.total)}</p>
                    <p><strong>Memory Freed:</strong> ${formatBytes(result.freed_memory)}</p>
                    <p><strong>Optimizations Performed:</strong></p>
                    ${optimizationsList}
                    <p><strong>Status:</strong> ${result.message}</p>
                `;
                
                resultDiv.style.display = 'block';
                
                const notificationMsg = result.freed_memory > 0
                    ? `Memory optimization complete! Freed ${formatBytes(result.freed_memory)}`
                    : 'Memory optimization complete!';
                showNotification(notificationMsg, 'success');
                
                await loadMemoryInfo();
            } catch (error) {
                console.error('Error optimizing memory:', error);
                showNotification('Memory optimization failed: ' + error, 'error');
            }
        }
    });
    
    // Setup all event listeners
    setupEventListeners();
    console.log('Event listeners setup complete');
    
    // Add a fallback setup after a short delay to ensure DOM is ready
    setTimeout(() => {
        const cleanBtn = document.getElementById('clean-selected');
        if (cleanBtn && !cleanBtn.hasAttribute('data-listener-attached')) {
            console.log('Attaching fallback listener to clean-selected button');
            cleanBtn.setAttribute('data-listener-attached', 'true');
            cleanBtn.onclick = (e) => {
                console.log('Clean button clicked via onclick handler');
                e.preventDefault();
                cleanSelectedFiles();
            };
        }
    }, 500);
    
    // Debug: Check if buttons exist in DOM and ensure they're properly styled
    setTimeout(() => {
        const deepCleanBtn = document.getElementById('deep-clean-memory');
        const optimizeBtn = document.getElementById('optimize-memory');
        console.log('=== Button Debug Info ===');
        console.log('Deep clean button found:', !!deepCleanBtn);
        console.log('Optimize button found:', !!optimizeBtn);
        if (deepCleanBtn) {
            console.log('Deep clean button parent:', deepCleanBtn.parentElement?.className);
            console.log('Deep clean button visible:', deepCleanBtn.offsetParent !== null);
            console.log('Deep clean button disabled:', deepCleanBtn.disabled);
            // Ensure button is clickable
            deepCleanBtn.style.cursor = 'pointer';
            deepCleanBtn.style.pointerEvents = 'auto';
        }
        if (optimizeBtn) {
            console.log('Optimize button parent:', optimizeBtn.parentElement?.className);
            console.log('Optimize button visible:', optimizeBtn.offsetParent !== null);
            console.log('Optimize button disabled:', optimizeBtn.disabled);
            // Ensure button is clickable
            optimizeBtn.style.cursor = 'pointer';
            optimizeBtn.style.pointerEvents = 'auto';
        }
        console.log('========================');
        console.log('Event delegation is active. Try clicking the Deep Clean button!');
    }, 1000);
    
    // Check if Tauri is available
    if (window.__TAURI__) {
        console.log('Tauri is available');
        
        // Initial load
        loadDashboard();
        
        // Auto-refresh dashboard every 5 seconds
        setInterval(() => {
            const activeTab = document.querySelector('.tab-content.active');
            if (activeTab && activeTab.id === 'dashboard') {
                loadDashboard();
            }
        }, 5000);
    } else {
        console.error('Tauri API not available');
        showNotification('Failed to initialize Tauri API', 'error');
    }
});
