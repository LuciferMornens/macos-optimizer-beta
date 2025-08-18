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

// Tab navigation
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
        
        // Load tab-specific data
        switch(tabName) {
            case 'dashboard':
                loadDashboard();
                break;
            case 'memory':
                loadMemoryInfo();
                break;
            case 'processes':
                loadProcesses();
                break;
        }
    });
});

// Dashboard functions
async function loadDashboard() {
    console.log('Loading dashboard...');
    try {
        // Load system info
        const systemInfo = await invoke('get_system_info');
        console.log('System info:', systemInfo);
        const memoryInfo = await invoke('get_memory_info');
        console.log('Memory info:', memoryInfo);
        const cpuInfo = await invoke('get_cpu_info');
        console.log('CPU info:', cpuInfo);
        const disks = await invoke('get_disks');
        console.log('Disks:', disks);
        
        // Update memory stats
        const memoryPercent = (memoryInfo.used_memory / memoryInfo.total_memory * 100).toFixed(1);
        document.getElementById('memory-usage').textContent = `${memoryPercent}%`;
        document.getElementById('memory-detail').textContent = 
            `${formatBytes(memoryInfo.used_memory)} / ${formatBytes(memoryInfo.total_memory)}`;
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
        const memoryInfo = await invoke('get_memory_info');
        
        document.getElementById('total-memory').textContent = formatBytes(memoryInfo.total_memory);
        document.getElementById('used-memory').textContent = formatBytes(memoryInfo.used_memory);
        document.getElementById('available-memory').textContent = formatBytes(memoryInfo.available_memory);
        document.getElementById('memory-pressure').textContent = `${memoryInfo.memory_pressure.toFixed(1)}%`;
        document.getElementById('swap-used').textContent = formatBytes(memoryInfo.used_swap);
        
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
            
            const usedAngle = (memoryInfo.used_memory / memoryInfo.total_memory) * Math.PI * 2;
            
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
            ctx.fillText(`${memoryInfo.memory_pressure.toFixed(0)}%`, centerX, centerY);
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

async function loadProcesses() {
    try {
        allProcesses = await invoke('get_processes');
        displayProcesses(allProcesses);
    } catch (error) {
        console.error('Error loading processes:', error);
        showNotification('Failed to load processes', 'error');
    }
}

function displayProcesses(processes) {
    const processList = document.getElementById('processes-list');
    processList.innerHTML = '';
    
    // Sort by memory by default
    processes.sort((a, b) => b.memory_usage - a.memory_usage);
    
    // Display top 50 processes
    processes.slice(0, 50).forEach(process => {
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
            const pid = parseInt(e.target.dataset.pid);
            const name = e.target.dataset.name;
            
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

// Search and sort processes
document.getElementById('process-search').addEventListener('input', (e) => {
    const searchTerm = e.target.value.toLowerCase();
    const filtered = allProcesses.filter(p => 
        p.name.toLowerCase().includes(searchTerm)
    );
    displayProcesses(filtered);
});

document.getElementById('process-sort').addEventListener('change', (e) => {
    const sortBy = e.target.value;
    let sorted = [...allProcesses];
    
    switch(sortBy) {
        case 'memory':
            sorted.sort((a, b) => b.memory_usage - a.memory_usage);
            break;
        case 'cpu':
            sorted.sort((a, b) => b.cpu_usage - a.cpu_usage);
            break;
        case 'name':
            sorted.sort((a, b) => a.name.localeCompare(b.name));
            break;
    }
    
    displayProcesses(sorted);
});

// Event listeners
document.getElementById('quick-optimize').addEventListener('click', async () => {
    try {
        showNotification('Starting optimization...', 'success');
        
        // Optimize memory
        const result = await invoke('optimize_memory');
        
        const freedMemory = formatBytes(Math.abs(result.freed_memory));
        showNotification(`Optimization complete! Freed ${freedMemory} of memory`, 'success');
        
        // Reload dashboard
        await loadDashboard();
    } catch (error) {
        console.error('Error optimizing:', error);
        showNotification('Optimization failed. Some operations may require admin privileges.', 'error');
    }
});

document.getElementById('optimize-memory').addEventListener('click', async () => {
    try {
        const result = await invoke('optimize_memory');
        
        const resultDiv = document.getElementById('optimization-result');
        const resultContent = resultDiv.querySelector('.result-content');
        
        resultContent.innerHTML = `
            <p><strong>Memory Before:</strong> ${formatBytes(result.memory_before.used)} / ${formatBytes(result.memory_before.total)}</p>
            <p><strong>Memory After:</strong> ${formatBytes(result.memory_after.used)} / ${formatBytes(result.memory_after.total)}</p>
            <p><strong>Memory Freed:</strong> ${formatBytes(result.freed_memory)}</p>
            <p><strong>Status:</strong> ${result.message}</p>
        `;
        
        resultDiv.style.display = 'block';
        showNotification('Memory optimization complete!', 'success');
        
        // Reload memory info
        await loadMemoryInfo();
    } catch (error) {
        console.error('Error optimizing memory:', error);
        showNotification('Memory optimization failed', 'error');
    }
});

document.getElementById('refresh-memory').addEventListener('click', loadMemoryInfo);
document.getElementById('refresh-processes').addEventListener('click', loadProcesses);
document.getElementById('scan-files').addEventListener('click', scanForCleanableFiles);
document.getElementById('clean-selected').addEventListener('click', cleanSelectedFiles);

// Notification close button
document.querySelector('.notification-close').addEventListener('click', () => {
    document.getElementById('notification').classList.remove('show');
});

// Wait for window to load and Tauri to be ready
window.addEventListener('DOMContentLoaded', () => {
    console.log('DOM loaded, checking Tauri...');
    
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
