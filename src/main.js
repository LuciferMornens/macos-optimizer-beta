const { invoke } = window.__TAURI__.core;

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

async function scanForCleanableFiles() {
    const scanProgress = document.getElementById('scan-progress');
    const cleaningReport = document.getElementById('cleaning-report');
    const cleanButton = document.getElementById('clean-selected');
    
    scanProgress.style.display = 'block';
    cleaningReport.style.display = 'none';
    cleanButton.disabled = true;
    
    try {
        const report = await invoke('scan_cleanable_files');
        cleanableFiles = await invoke('get_cleanable_files');
        
        // Update summary
        document.getElementById('total-cleanable').textContent = formatBytes(report.total_size);
        document.getElementById('files-found').textContent = report.files_count;
        
        // Display categories
        const categoriesList = document.getElementById('categories-list');
        categoriesList.innerHTML = '';
        
        report.categories.forEach(category => {
            const categoryCard = document.createElement('div');
            categoryCard.className = 'category-card';
            categoryCard.innerHTML = `
                <div class="category-name">${category.name}</div>
                <div class="category-size">${formatBytes(category.size)} (${category.count} files)</div>
            `;
            categoryCard.addEventListener('click', () => filterFilesByCategory(category.name));
            categoriesList.appendChild(categoryCard);
        });
        
        // Display files
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

function displayFiles(files) {
    const filesList = document.getElementById('files-list');
    filesList.innerHTML = '';
    
    files.slice(0, 100).forEach((file, index) => {
        const fileItem = document.createElement('div');
        fileItem.className = 'file-item';
        fileItem.innerHTML = `
            <input type="checkbox" id="file-${index}" value="${file.path}" 
                   ${file.safe_to_delete ? '' : 'disabled'}>
            <div class="file-info">
                <div class="file-path">${file.path}</div>
                <div class="file-size">${formatBytes(file.size)} - ${file.description}</div>
            </div>
        `;
        filesList.appendChild(fileItem);
    });
}

function filterFilesByCategory(categoryName) {
    const filtered = cleanableFiles.filter(file => file.category === categoryName);
    displayFiles(filtered);
    
    // Update category selection
    document.querySelectorAll('.category-card').forEach(card => {
        if (card.querySelector('.category-name').textContent === categoryName) {
            card.classList.add('selected');
        } else {
            card.classList.remove('selected');
        }
    });
}

async function cleanSelectedFiles() {
    const selectedFiles = [];
    document.querySelectorAll('#files-list input[type="checkbox"]:checked').forEach(checkbox => {
        selectedFiles.push(checkbox.value);
    });
    
    if (selectedFiles.length === 0) {
        showNotification('No files selected for cleaning', 'error');
        return;
    }
    
    if (!confirm(`Are you sure you want to delete ${selectedFiles.length} files?`)) {
        return;
    }
    
    try {
        const [freedBytes, filesDeleted] = await invoke('clean_files', { filePaths: selectedFiles });
        showNotification(`Cleaned ${filesDeleted} files, freed ${formatBytes(freedBytes)}`, 'success');
        
        // Rescan after cleaning
        await scanForCleanableFiles();
    } catch (error) {
        console.error('Error cleaning files:', error);
        showNotification('Failed to clean selected files', 'error');
    }
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

            if (confirm(`Are you sure you want to end the process "${name}" (PID: ${pid})?`)) {
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
    
    const cleanSelectedBtn = document.getElementById('clean-selected');
    if (cleanSelectedBtn) {
        cleanSelectedBtn.addEventListener('click', cleanSelectedFiles);
    }
    
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
    
    // Use event delegation for ALL button clicks (works with dynamic content)
    document.body.addEventListener('click', async (event) => {
        // Guard against non-Element targets (text nodes in WebKit)
        const tgt = event.target;
        if (!(tgt instanceof Element)) return;
        
        // Check if clicked element is a button or inside a button
        const button = tgt.closest('button');
        if (!button) return;
        
        console.log('Button clicked with ID:', button.id);
        
        // Skip deep-clean-memory as it has its own direct listener
        if (button.id === 'deep-clean-memory') {
            console.log('Deep clean button - handled by direct listener');
            return;
        }
        
        // Handle optimize memory button
        if (button.id === 'optimize-memory') {
            console.log('Optimize memory button handler triggered!');
            event.preventDefault();
            
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
    console.log('Event delegation attached');
    
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
