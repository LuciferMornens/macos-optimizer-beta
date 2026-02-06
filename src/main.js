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

const TOAST_ICONS = {
    success: '<svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><circle cx="10" cy="10" r="8.5" stroke="currentColor" stroke-width="1.6"/><path d="M6 10.5l2.2 2.2 4.4-5.4" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>',
    error: '<svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><circle cx="10" cy="10" r="8.5" stroke="currentColor" stroke-width="1.6"/><path d="M7.2 7.2l5.6 5.6M12.8 7.2l-5.6 5.6" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>',
    warning: '<svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="M10 2.5l7.5 13a1 1 0 0 1-.87 1.5H3.37a1 1 0 0 1-.87-1.5l7.5-13a1 1 0 0 1 1.74 0Z" stroke="currentColor" stroke-width="1.4" fill="none"/><path d="M10 7v4.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><circle cx="10" cy="14.5" r="1" fill="currentColor"/></svg>',
    info: '<svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><circle cx="10" cy="10" r="8.5" stroke="currentColor" stroke-width="1.6"/><path d="M10 8.5v5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><circle cx="10" cy="6" r="1" fill="currentColor"/></svg>'
};

function resolveToastIcon(type) {
    return TOAST_ICONS[type] || TOAST_ICONS.info;
}

function showNotification(message, type = 'info', options = {}) {
    const { duration = 4200, dismissible = true } = options;
    if (!message) return;

    let stack = document.getElementById('toast-stack');
    if (!stack) {
        stack = document.createElement('div');
        stack.id = 'toast-stack';
        stack.className = 'toast-stack';
        stack.setAttribute('aria-live', 'polite');
        stack.setAttribute('aria-atomic', 'true');
        document.body.appendChild(stack);
    }

    // Limit number of concurrent toasts to keep UI tidy
    while (stack.children.length >= 4) {
        stack.removeChild(stack.firstElementChild);
    }

    const toast = document.createElement('div');
    toast.className = `toast toast--${type}`;
    toast.setAttribute('role', type === 'error' ? 'alert' : 'status');

    const icon = document.createElement('span');
    icon.className = 'toast__icon';
    icon.innerHTML = resolveToastIcon(type);

    const body = document.createElement('div');
    body.className = 'toast__body';
    body.textContent = message;

    const progress = document.createElement('div');
    progress.className = 'toast__progress';
    const bar = document.createElement('span');
    bar.style.animationDuration = `${Math.max(duration, 0)}ms`;
    progress.appendChild(bar);

    let closeBtn = null;
    if (dismissible) {
        closeBtn = document.createElement('button');
        closeBtn.className = 'toast__close';
        closeBtn.type = 'button';
        closeBtn.setAttribute('aria-label', 'Dismiss notification');
        closeBtn.textContent = '×';
    }

    const removeToast = () => {
        toast.classList.add('toast--exit');
        toast.addEventListener('animationend', () => toast.remove(), { once: true });
    };

    if (closeBtn) {
        closeBtn.addEventListener('click', () => removeToast());
    }

    toast.append(icon, body);
    if (closeBtn) {
        toast.append(closeBtn);
    }
    toast.append(progress);
    stack.appendChild(toast);

    if (duration > 0) {
        setTimeout(removeToast, duration);
    }
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

// Skeleton loader functions
function showDashboardSkeleton() {
    const skeleton = document.getElementById('dashboard-skeleton');
    const statsGrid = document.querySelector('#dashboard .stats-grid');
    if (skeleton && statsGrid) {
        skeleton.style.display = 'block';
        statsGrid.style.opacity = '0.3';
    }
}

function hideDashboardSkeleton() {
    const skeleton = document.getElementById('dashboard-skeleton');
    const statsGrid = document.querySelector('#dashboard .stats-grid');
    if (skeleton && statsGrid) {
        skeleton.style.display = 'none';
        statsGrid.style.opacity = '1';
    }
}

function showGlobalLoading(message = 'Processing...') {
    const overlay = document.getElementById('global-loading');
    const text = overlay?.querySelector('.loading-text');
    if (overlay) {
        if (text) text.textContent = message;
        overlay.classList.add('show');
    }
}

function hideGlobalLoading() {
    const overlay = document.getElementById('global-loading');
    if (overlay) {
        overlay.classList.remove('show');
    }
}

// Dashboard functions  
async function loadDashboard() {
    console.log('Loading dashboard...');
    
    // Show skeleton for perceived performance
    showDashboardSkeleton();
    
    try {
        const [metricsResult, systemInfoResult] = await Promise.allSettled([
            invoke('get_metrics_snapshot'),
            invoke('get_system_info')
        ]);

        const unwrapSample = (label, envelope) => {
            if (!envelope) return null;
            if (envelope.value) return envelope.value;
            if (envelope.error) {
                console.warn(`${label} sample unavailable: ${envelope.error}`);
            }
            return null;
        };

        let bootTimeOverride = null;

        if (metricsResult.status === 'fulfilled') {
            const snapshot = metricsResult.value;
            const memoryStats = unwrapSample('memory', snapshot.memory);
            const cpuStats = unwrapSample('cpu', snapshot.cpu);
            const diskStats = unwrapSample('disk', snapshot.disks);
            const uptimeStats = unwrapSample('uptime', snapshot.uptime);

            if (memoryStats) {
                const memoryRatio = memoryStats.total > 0 ? (memoryStats.used / memoryStats.total) : 0;
                const memoryPercent = (memoryRatio * 100).toFixed(1);
                document.getElementById('memory-usage').textContent = `${memoryPercent}%`;
                document.getElementById('memory-detail').textContent =
                    `${formatBytes(memoryStats.used)} / ${formatBytes(memoryStats.total)}`;
                document.getElementById('memory-progress').style.width = `${memoryPercent}%`;
            }

            if (cpuStats) {
                document.getElementById('cpu-usage').textContent = `${cpuStats.total_usage.toFixed(1)}%`;
                document.getElementById('cpu-detail').textContent = `${cpuStats.core_count} cores`;
                document.getElementById('cpu-progress').style.width = `${cpuStats.total_usage.toFixed(1)}%`;
            }

            if (diskStats && Array.isArray(diskStats) && diskStats.length > 0) {
                const primaryDisk = diskStats.find(d => d.is_system) || diskStats[0];
                const diskPercent = primaryDisk.total_space > 0
                    ? ((primaryDisk.used_space / primaryDisk.total_space) * 100).toFixed(1)
                    : '0.0';
                document.getElementById('disk-usage').textContent = `${diskPercent}%`;
                document.getElementById('disk-detail').textContent =
                    `${formatBytes(primaryDisk.used_space)} / ${formatBytes(primaryDisk.total_space)}`;
                document.getElementById('disk-progress').style.width = `${diskPercent}%`;
            }

            if (uptimeStats) {
                bootTimeOverride = uptimeStats.boot_time_seconds;
                document.getElementById('uptime').textContent = formatUptime(uptimeStats.uptime_seconds);
                document.getElementById('boot-time').textContent =
                    new Date(uptimeStats.boot_time_seconds * 1000).toLocaleString();
            }
        } else {
            console.error('Metrics snapshot failed:', metricsResult.reason);
            showNotification('Failed to load metrics snapshot', 'error');
        }

        if (systemInfoResult.status === 'fulfilled') {
            const sysInfo = systemInfoResult.value;
            document.getElementById('system-info').textContent = sysInfo.os_name;
            document.getElementById('os-version').textContent = sysInfo.os_version;
            document.getElementById('hostname').textContent = sysInfo.hostname;
            if (bootTimeOverride === null) {
                document.getElementById('uptime').textContent = formatUptime(sysInfo.uptime);
            }
            const bootSeconds = bootTimeOverride ?? sysInfo.boot_time;
            document.getElementById('boot-time').textContent =
                new Date(bootSeconds * 1000).toLocaleString();
        } else if (systemInfoResult.status === 'rejected') {
            console.error('System info failed:', systemInfoResult.reason);
        }

    } catch (error) {
        console.error('Error loading dashboard:', error);
        showNotification('Failed to load dashboard data', 'error');
    } finally {
        // Hide skeleton after loading
        setTimeout(() => {
            hideDashboardSkeleton();
        }, 300);
    }
}

// Memory management functions
async function loadMemoryInfo() {
    try {
        const snapshot = await invoke('get_metrics_snapshot');
        const envelope = snapshot.memory;
        if (!envelope || !envelope.value) {
            const reason = envelope?.error || 'memory metrics unavailable';
            throw new Error(reason);
        }

        const stats = envelope.value;
        const usedRatio = stats.total > 0 ? (stats.used / stats.total) : 0;
        
        document.getElementById('total-memory').textContent = formatBytes(stats.total);
        document.getElementById('used-memory').textContent = formatBytes(stats.used);
        document.getElementById('available-memory').textContent = formatBytes(stats.available);
        document.getElementById('memory-pressure').textContent = `${stats.pressure_percent.toFixed(1)}% (${stats.pressure_state})`;
        document.getElementById('swap-used').textContent = `${formatBytes(stats.swap_used)} / ${formatBytes(stats.swap_total)}`;
        
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
const RISK_MODE_STORAGE_KEY = 'storageCleaner.allowRisky';
const RISK_DESCRIPTION_DEFAULT = 'Allow manual selection of low-safety items after acknowledging the risks. Files are always moved to the Trash first.';
const RISK_DESCRIPTION_EMPTY = 'All scanned items are currently considered safe. Risky Mode is optional right now.';
let allowRiskySelections = false;
let categorySafetySummary = new Map();

async function scanForCleanableFiles() {
    const scanProgress = document.getElementById('scan-progress');
    const cleaningReport = document.getElementById('cleaning-report');
    const cleanButton = document.getElementById('clean-selected');
    
    return operationQueue.add(
        async () => {
            scanProgress.style.display = '';
            cleaningReport.style.display = 'none';
            cleanButton.disabled = true;
            
            try {
                const report = await invoke('scan_cleanable_files_enhanced');
                lastReport = report.base || report;
                const enhanced = (report.enhanced_files || []).map(f => f.base);
                cleanableFiles = enhanced && enhanced.length > 0 ? enhanced : await invoke('get_cleanable_files');
                categorySafetySummary = computeCategorySummaries(cleanableFiles);
                
                // Update summary (prefer base fields from enhanced scan)
                const totalSize = (report && report.base && typeof report.base.total_size === 'number') ? report.base.total_size : (report && typeof report.total_size === 'number') ? report.total_size : 0;
                const filesCount = (report && report.base && typeof report.base.files_count === 'number') ? report.base.files_count : (report && typeof report.files_count === 'number') ? report.files_count : 0;
                document.getElementById('total-cleanable').textContent = formatBytes(totalSize);
                document.getElementById('files-found').textContent = filesCount;
                
                // Display categories
                renderCategories();
                
                // Display files
                currentCategoryFilter = null;
                displayFiles(cleanableFiles);

                const hasRisky = Array.from(categorySafetySummary.values()).some(entry => entry.riskyCount > 0);
                updateRiskModeBannerState(hasRisky);

                scanProgress.style.display = 'none';
                cleaningReport.style.display = 'block';
                cleanButton.disabled = false;
                
                showNotification(`Found ${filesCount} cleanable files (${formatBytes(totalSize)})`, 'success');
                return report;
            } catch (error) {
                console.error('Error scanning files:', error);
                showNotification('Failed to scan for cleanable files', 'error');
                categorySafetySummary = new Map();
                updateRiskModeBannerState(false);
                scanProgress.style.display = 'none';
                throw error;
            }
        },
        {
            debounce: 1000,
            id: 'scan-files',
            priority: 1,
            description: 'File System Scan',
            timeout: 240000,
        }
    );
}

function computeCategorySummaries(files) {
    const summary = new Map();
    files.forEach(file => {
        const entry = summary.get(file.category) || { total: 0, safeCount: 0, riskyCount: 0, maxScore: 0 };
        entry.total += 1;
        if (file.safe_to_delete) {
            entry.safeCount += 1;
        } else {
            entry.riskyCount += 1;
        }
        if (typeof file.safety_score === 'number') {
            entry.maxScore = Math.max(entry.maxScore, file.safety_score || 0);
        }
        summary.set(file.category, entry);
    });
    return summary;
}

function updateRiskModeBannerState(hasRiskyItems) {
    const banner = document.getElementById('risk-mode-banner');
    if (!banner) return;
    banner.classList.toggle('risk-mode-banner--active', allowRiskySelections);
    banner.classList.toggle('risk-mode-banner--no-risk', !hasRiskyItems);
    const description = banner.querySelector('.risk-mode-description');
    if (description) {
        description.textContent = hasRiskyItems ? RISK_DESCRIPTION_DEFAULT : RISK_DESCRIPTION_EMPTY;
    }
    const toggleLabel = banner.querySelector('.risk-mode-toggle-label');
    if (toggleLabel) {
        toggleLabel.textContent = allowRiskySelections ? 'Risky mode on' : 'Risky mode off';
    }
    const toggleInput = banner.querySelector('#enable-risk-mode');
    if (toggleInput && toggleInput.checked !== allowRiskySelections) {
        toggleInput.checked = allowRiskySelections;
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
            const summary = categorySafetySummary.get(category.name) || null;
            const card = createCategoryCard(category.name, category.size, category.count, false, summary);
            categoriesList.appendChild(card);
        }
    });

    if (showAdvanced) {
        // Render advanced that have items from the report
        lastReport.categories.forEach(category => {
            const isAdvanced = advancedSet.has(category.name.toLowerCase()) || /\(advanced\)/i.test(category.name);
            if (isAdvanced) {
                const summary = categorySafetySummary.get(category.name) || null;
                const card = createCategoryCard(category.name, category.size, category.count, true, summary);
                categoriesList.appendChild(card);
            }
        });

        // Also render advanced categories that exist in rules but had zero results
        (lastReport.advanced_categories || []).forEach(name => {
            if (!present.has(name.toLowerCase())) {
                const summary = categorySafetySummary.get(name) || null;
                const card = createCategoryCard(name, 0, 0, true, summary);
                categoriesList.appendChild(card);
            }
        });
    }
}

function createCategoryCard(name, size, count, isAdvanced = false, summary = null) {
    const categoryCard = document.createElement('div');
    categoryCard.className = 'category-card';
    categoryCard.dataset.name = name;
    if (count === 0) {
        categoryCard.classList.add('category-card--empty');
    }

    const badges = [];
    if (isAdvanced) {
        badges.push('<span class="badge badge-advanced">Advanced</span>');
    }
    if (summary && summary.riskyCount > 0) {
        const label = summary.riskyCount > 1 ? `Risky (${summary.riskyCount})` : 'Risky';
        badges.push(`<span class="badge badge-risk">${label}</span>`);
        categoryCard.classList.add('category-card--risky');
    }
    const badgesMarkup = badges.length ? `<span class="category-badges">${badges.join(' ')}</span>` : '';

    categoryCard.innerHTML = `
        <div class="category-name">${name}${badgesMarkup ? ` ${badgesMarkup}` : ''}</div>
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

        const isRisky = !file.safe_to_delete;
        const disabled = isRisky && !allowRiskySelections;
        if (isRisky) {
            fileItem.classList.add('file-item--risky');
            if (allowRiskySelections) {
                fileItem.classList.add('file-item--risky-enabled');
            }
        }

        // Determine safety badge colour & label
        let safetyClass = 'safety-low';
        let safetyText = 'Low';
        if (isRisky) {
            safetyClass = allowRiskySelections ? 'safety-risk' : 'safety-blocked';
            safetyText = allowRiskySelections ? 'Risky' : 'Blocked';
        } else if (file.safety_score >= 95) {
            safetyClass = 'safety-very-high';
            safetyText = 'Very Safe';
        } else if (file.safety_score >= 80) {
            safetyClass = 'safety-high';
            safetyText = 'Safe';
        } else if (file.safety_score >= 60) {
            safetyClass = 'safety-medium';
            safetyText = 'Review';
        }

        const checkboxAttrs = [
            `id="file-${index}"`,
            `value="${file.path}"`,
            `data-size="${file.size}"`,
            `data-risky="${isRisky}"`,
            disabled ? 'disabled' : '',
            file.auto_select && file.safe_to_delete ? 'checked' : ''
        ].filter(Boolean).join(' ');

        fileItem.innerHTML = `
            <input type="checkbox" ${checkboxAttrs}>
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
                ${isRisky ? '<div class="file-warning">Flagged for manual review. Enable Risky Mode to include this item.</div>' : ''}
            </div>
        `;

        if (isRisky && allowRiskySelections) {
            const warning = fileItem.querySelector('.file-warning');
            if (warning) {
                warning.textContent = 'Marked as risky. Review details before deleting.';
            }
        }

        filesList.appendChild(fileItem);
    });
    
    // Update selection count after displaying files
    updateSelectionInfo();
}

function refreshDisplayedFiles() {
    if (currentCategoryFilter) {
        const filtered = cleanableFiles.filter(file => file.category === currentCategoryFilter);
        displayFiles(filtered);
    } else {
        displayFiles(cleanableFiles);
    }
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
    const files = cleanableFiles.filter(f => f.category === categoryName && (f.safe_to_delete || allowRiskySelections));
    if (files.length === 0) {
        const summary = categorySafetySummary.get(categoryName);
        if (summary && summary.riskyCount > 0 && !allowRiskySelections) {
            showNotification('Enable Risky Mode to include items flagged for manual review.', 'warning');
        } else {
            showNotification('No cleanable files found in this category.', 'warning');
        }
        return;
    }
    const totalSize = files.reduce((acc, f) => acc + (f.size || 0), 0);
    const riskyCount = files.filter(f => !f.safe_to_delete).length;
    const confirmed = await userConfirm(
        `Clean ${files.length} files in "${categoryName}"?\n\nThis will free approximately ${formatBytes(totalSize)}${riskyCount > 0 ? `\n\nWARNING: ${riskyCount} item(s) are flagged as risky and will be moved to the Trash.` : ''}`,
        { title: 'Clean Category', kind: 'warning' }
    );
    if (!confirmed) return;

    try {
        const result = await operationQueue.add(
            () =>
                invoke('clean_files_enhanced', {
                    filePaths: files.map(f => f.path),
                    allowLowSafety: allowRiskySelections,
                }),
            {
                description: `Enhanced File Clean (${categoryName})`,
                priority: 1,
                timeout: null,
            }
        );
        const freedBytes = result.total_freed || 0;
        const filesDeleted = result.deleted_count || 0;
        const skipped = Array.isArray(result.failed_files) ? result.failed_files : [];

        if (filesDeleted > 0) {
            showNotification(`Cleaned ${filesDeleted} files, freed ${formatBytes(freedBytes)}`, 'success');
        } else {
            showNotification('No files were deleted. Review skipped items for more details.', 'info');
        }

        if (skipped.length > 0) {
            const summary = skipped
                .slice(0, 3)
                .map(item => {
                    const name = item.path?.split('/').filter(Boolean).pop() || item.path;
                    return `${name}: ${item.reason}`;
                })
                .join(' • ');
            showNotification(`Skipped ${skipped.length} item(s): ${summary}`, 'warning');
        }
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
    
    const selectedSet = new Set(selectedFiles);
    const riskySelections = cleanableFiles.filter(file => selectedSet.has(file.path) && !file.safe_to_delete);
    if (riskySelections.length > 0 && !allowRiskySelections) {
        showNotification('Risky items are blocked. Enable Risky Mode to include them.', 'warning');
        return;
    }

    // Pre-validate with enhanced pipeline to surface warnings/errors
    let confirmMessage = `Are you sure you want to delete ${selectedFiles.length} files?\n\nThis will free approximately ${formatBytes(totalSize)}`;
    if (riskySelections.length > 0) {
        confirmMessage += `\n\nWARNING: ${riskySelections.length} item(s) are flagged as risky and will be moved to the Trash. Review them carefully.`;
    }
    try {
        const prep = await invoke('prepare_deletion_enhanced', { filePaths: selectedFiles });
        if (prep && prep.validation_result) {
            const warnings = (prep.validation_result.warnings || []).slice(0, 5);
            const errors = (prep.validation_result.errors || []).slice(0, 5);
            if (errors.length > 0) {
                confirmMessage += `\n\nBlocked (${errors.length}):`;
                errors.forEach(e => {
                    confirmMessage += `\n• ${e.message}`;
                });
            }
            if (warnings.length > 0) {
                confirmMessage += `\n\nWarnings (${warnings.length}):`;
                warnings.forEach(w => {
                    confirmMessage += `\n• ${w.message}`;
                });
            }
        }
    } catch (e) {
        console.warn('Enhanced pre-validation failed, proceeding with confirmation only', e);
    }

    const confirmed = await userConfirm(confirmMessage, { title: 'Confirm Clean', kind: 'warning' });
    if (!confirmed) {
        console.log('User cancelled file cleaning');
        return;
    }
    
    console.log('User confirmed, starting file cleaning...');
    
    try {
        showNotification('Cleaning selected files...', 'info');
        const result = await operationQueue.add(
            () =>
                invoke('clean_files_enhanced', {
                    filePaths: selectedFiles,
                    allowLowSafety: allowRiskySelections,
                }),
            {
                description: 'Enhanced File Clean (Selection)',
                priority: 1,
                timeout: null,
            }
        );
        const freedBytes = result.total_freed || 0;
        const filesDeleted = result.deleted_count || 0;
        const skipped = Array.isArray(result.failed_files) ? result.failed_files : [];

        console.log(`Clean complete: ${filesDeleted} files deleted, ${freedBytes} bytes freed`);

        if (filesDeleted > 0) {
            showNotification(`Cleaned ${filesDeleted} files, freed ${formatBytes(freedBytes)}`, 'success');
        } else {
            showNotification('No files were deleted. Review skipped items for more details.', 'info');
        }

        if (skipped.length > 0) {
            const summary = skipped
                .slice(0, 3)
                .map(item => {
                    const name = item.path?.split('/').filter(Boolean).pop() || item.path;
                    return `${name}: ${item.reason}`;
                })
                .join(' • ');
            showNotification(`Skipped ${skipped.length} item(s): ${summary}`, 'warning');
        }
        
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
            
            // Check if button is already disabled/loading
            if (deepCleanBtn.disabled || deepCleanBtn.classList.contains('loading')) {
                return;
            }
            
            const confirmed = await userConfirm(
                '⚠️ Deep Clean with Administrator Access\n\n' +
                'This will:\n' +
                '• Purge all disk caches\n' +
                '• Clear DNS and network caches\n' +
                '• Optimize memory compression\n' +
                '• Free inactive memory\n' +
                '• Clear application caches\n\n' +
                'You will be prompted for your administrator password.\n' +
                'Continue?',
                { title: 'Deep Clean (Admin)', kind: 'warning' }
            );
            
            if (!confirmed) {
                console.log('User cancelled deep clean');
                return;
            }
            
            try {
                deepCleanBtn.disabled = true;
                deepCleanBtn.classList.add('loading');
                
                await operationQueue.add(
                    async () => {
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
                        
                        await loadMemoryInfo();
                        return result;
                    },
                    { 
                        debounce: 500, 
                        id: 'deep-clean-memory', 
                        priority: 3,
                        description: 'Deep Memory Clean (Admin)',
                        timeout: 20000
                    }
                );
            } catch (error) {
                console.error('Deep clean error:', error);
                showNotification('Deep clean failed: ' + error, 'error');
            } finally {
                deepCleanBtn.disabled = false;
                deepCleanBtn.classList.remove('loading');
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

    // Basic restore from Trash action (restores selected file names into Downloads)
    const restoreBtn = document.getElementById('restore-from-trash');
    if (restoreBtn) {
        restoreBtn.addEventListener('click', async () => {
            const namesInput = prompt('Enter exact Trash item names to restore (comma-separated).');
            if (!namesInput) return;
            const names = namesInput.split(',').map(s => s.trim()).filter(Boolean);
            if (names.length === 0) return;
            try {
                const restored = await invoke('restore_from_trash', { fileNames: names });
                showNotification(`Restored ${restored} item(s) to Downloads`, 'success');
            } catch (e) {
                console.error('Restore from Trash failed:', e);
                showNotification('Failed to restore items from Trash', 'error');
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

    const riskToggle = document.getElementById('enable-risk-mode');
    const initialHasRisky = Array.from(categorySafetySummary.values()).some(entry => entry.riskyCount > 0);
    if (riskToggle) {
        const storedRiskSetting = localStorage.getItem(RISK_MODE_STORAGE_KEY);
        allowRiskySelections = storedRiskSetting === 'true';
        riskToggle.checked = allowRiskySelections;
        updateRiskModeBannerState(initialHasRisky);

        riskToggle.addEventListener('change', async (event) => {
            if (event.target.checked) {
                const confirmed = await userConfirm(
                    'Risky Mode allows manual selection of files flagged as low safety. They will be moved to the Trash first, but deleting them can affect apps or personal data. Continue?',
                    { title: 'Enable Risky Mode', kind: 'warning' }
                );
                if (!confirmed) {
                    event.target.checked = false;
                    return;
                }
                allowRiskySelections = true;
                localStorage.setItem(RISK_MODE_STORAGE_KEY, 'true');
                showNotification('Risky Mode enabled. Low-safety items can now be selected manually.', 'warning', { duration: 5200 });
            } else {
                allowRiskySelections = false;
                localStorage.removeItem(RISK_MODE_STORAGE_KEY);
                document.querySelectorAll('#files-list input[type="checkbox"][data-risky="true"]').forEach(checkbox => {
                    checkbox.checked = false;
                    checkbox.disabled = true;
                });
                updateSelectionInfo();
                showNotification('Risky Mode disabled. Low-safety items are blocked from selection.', 'info');
            }

            const hasRisky = Array.from(categorySafetySummary.values()).some(entry => entry.riskyCount > 0);
            updateRiskModeBannerState(hasRisky);
            refreshDisplayedFiles();
        });
    } else {
        allowRiskySelections = localStorage.getItem(RISK_MODE_STORAGE_KEY) === 'true';
        updateRiskModeBannerState(initialHasRisky);
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
            
            // Check if button is already disabled/loading
            if (button.disabled || button.classList.contains('loading')) {
                return;
            }
            
            const useAdmin = confirm(
                'Memory Optimization Options:\n\n' +
                'OK = Deep Optimization (requires admin password)\n' +
                'Cancel = Safe Mode (no admin required)\n\n' +
                'Deep optimization can free more memory but requires administrator access.'
            );
            
            // Add to operation queue with debouncing
            try {
                button.disabled = true;
                button.classList.add('loading');
                
                await operationQueue.add(
                    async () => {
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
                        
                        await loadMemoryInfo();
                        return result;
                    },
                    { 
                        debounce: 1000, 
                        id: 'optimize-memory', 
                        priority: 2,
                        description: useAdmin ? 'Deep Memory Optimization (Admin)' : 'Memory Optimization',
                        timeout: 15000
                    }
                );
            } catch (error) {
                console.error('Error optimizing memory:', error);
                showNotification('Memory optimization failed: ' + error, 'error');
            } finally {
                button.disabled = false;
                button.classList.remove('loading');
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
