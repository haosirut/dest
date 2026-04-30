// ═══════════════════════════════════════════════════════════════
// Соты — P2P Хранилище: Frontend Application Logic
// ═══════════════════════════════════════════════════════════════

const { invoke } = window.__TAURI__.core;

// ─── State ───────────────────────────────────────────────────────

const state = {
    initialized: false,
    isKeeper: false,
    calcDisk: 'hdd',
    topupMethod: 'card',
    timerSeconds: 300,
    timerInterval: null,
};

// ─── Helpers ─────────────────────────────────────────────────────

function $(sel) { return document.querySelector(sel); }
function $$(sel) { return document.querySelectorAll(sel); }

function formatMoney(n) {
    return n.toFixed(2).replace(/\B(?=(\d{3})+(?!\d))/g, ' ') + ' \u20BD';
}

function formatBytes(bytes) {
    if (bytes === 0) return '0 Б';
    const units = ['Б', 'КБ', 'МБ', 'ГБ', 'ТБ'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0) + ' ' + units[i];
}

function formatGB(gb) {
    if (gb < 0.001) return '0 ГБ';
    if (gb < 1) return (gb * 1024).toFixed(1) + ' МБ';
    return gb.toFixed(2) + ' ГБ';
}

function toast(msg, type = 'info') {
    const container = $('#toast-container');
    const el = document.createElement('div');
    el.className = `toast ${type}`;
    el.textContent = msg;
    container.appendChild(el);
    setTimeout(() => el.remove(), 3000);
}

async function safeInvoke(cmd, args = {}) {
    try {
        return await invoke(cmd, args);
    } catch (e) {
        console.error(`[${cmd}]`, e);
        toast(String(e), 'error');
        return null;
    }
}

// ─── Onboarding ──────────────────────────────────────────────────

async function init() {
    const isInit = await safeInvoke('check_initialized');
    if (isInit) {
        showApp();
    }
    startTimer();
}

$('#btn-create-wallet').addEventListener('click', async () => {
    const info = await safeInvoke('create_wallet');
    if (info) {
        $('#mnemonic-display').textContent = info.mnemonic;
        $('#peer-id-display').textContent = info.peer_id;
        $('#wallet-created').classList.remove('hidden');
        $('#onboard-actions').classList.add('hidden');
    }
});

$('#btn-restore-wallet').addEventListener('click', () => {
    $('#restore-form').classList.remove('hidden');
    $('#onboard-actions').classList.add('hidden');
});

$('#btn-cancel-restore').addEventListener('click', () => {
    $('#restore-form').classList.add('hidden');
    $('#onboard-actions').classList.remove('hidden');
});

$('#btn-do-restore').addEventListener('click', async () => {
    const mnemonic = $('#mnemonic-input').value.trim();
    if (!mnemonic) { toast('Введите фразу восстановления', 'error'); return; }
    const info = await safeInvoke('restore_wallet', { mnemonic });
    if (info) {
        $('#mnemonic-display').textContent = info.mnemonic;
        $('#peer-id-display').textContent = info.peer_id;
        $('#wallet-created').classList.remove('hidden');
        $('#restore-form').classList.add('hidden');
    }
});

$('#btn-enter-app').addEventListener('click', () => {
    showApp();
});

function showApp() {
    $('#onboarding').classList.add('hidden');
    $('#app-main').classList.remove('hidden');
    state.initialized = true;
    refreshAll();
}

// ─── Tab Navigation ──────────────────────────────────────────────

$$('.nav-btn').forEach(btn => {
    btn.addEventListener('click', () => {
        $$('.nav-btn').forEach(b => b.classList.remove('active'));
        $$('.tab').forEach(t => t.classList.remove('active'));
        btn.classList.add('active');
        $(`#tab-${btn.dataset.tab}`).classList.add('active');
    });
});

// ─── Refresh All Data ────────────────────────────────────────────

async function refreshAll() {
    await Promise.all([
        refreshBalances(),
        refreshFiles(),
        refreshKeeperStats(),
        refreshClientStats(),
        refreshPaymentHistory(),
        refreshReferralInfo(),
        refreshSettings(),
        refreshGeo(),
        updateCalculator(),
    ]);
}

// ─── Files ───────────────────────────────────────────────────────

const dropZone = $('#drop-zone');
const fileInput = $('#file-input');

$('#upload-btn').addEventListener('click', () => fileInput.click());

dropZone.addEventListener('dragover', e => { e.preventDefault(); dropZone.classList.add('dragover'); });
dropZone.addEventListener('dragleave', () => dropZone.classList.remove('dragover'));
dropZone.addEventListener('drop', e => {
    e.preventDefault();
    dropZone.classList.remove('dragover');
    handleFiles(e.dataTransfer.files);
});
fileInput.addEventListener('change', () => {
    handleFiles(fileInput.files);
    fileInput.value = '';
});

async function handleFiles(fileList) {
    const diskType = $('#disk-type-select').value;
    for (const file of fileList) {
        const result = await safeInvoke('upload_file', {
            name: file.name,
            sizeBytes: file.size,
            diskType,
        });
        if (result) {
            toast(`"${file.name}" загружен`, 'success');
        }
    }
    refreshFiles();
    refreshBalances();
    refreshClientStats();
    refreshPaymentHistory();
}

async function refreshFiles() {
    const files = await safeInvoke('get_files');
    const container = $('#file-list');

    if (!files || files.length === 0) {
        container.innerHTML = '<div class="empty-state"><p>Файлы отсутствуют</p><p class="empty-hint">Загрузите файлы, чтобы начать хранение</p></div>';
        return;
    }

    container.innerHTML = files.map(f => `
        <div class="file-item" data-id="${f.id}">
            <div class="file-icon">${getFileIcon(f.name)}</div>
            <div class="file-info">
                <div class="file-name">${escapeHtml(f.name)}</div>
                <div class="file-meta">
                    <span>${formatBytes(f.size_bytes)}</span>
                    <span>${f.disk_type.toUpperCase()}</span>
                    <span>${formatMoney(f.cost_per_month)}/мес</span>
                </div>
            </div>
            <div class="replica-badges" title="Репликация x${f.replicas}">
                ${'<span class="replica-dot"></span>'.repeat(Math.min(f.replicas, 4))}
            </div>
            <div class="file-cost">${formatMoney(f.cost_per_month)}/мес</div>
            <div class="file-actions">
                <button class="btn btn-sm btn-outline" onclick="downloadFile('${f.id}')">Скачать</button>
                <button class="btn btn-sm btn-danger" onclick="deleteFile('${f.id}', '${escapeHtml(f.name)}')">Удалить</button>
            </div>
        </div>
    `).join('');
}

function getFileIcon(name) {
    const ext = name.split('.').pop().toLowerCase();
    const icons = {
        pdf: '\uD83D\uDCC4', doc: '\uD83D\uDCC3', docx: '\uD83D\uDCC3',
        xls: '\uD83D\uDCC8', xlsx: '\uD83D\uDCC8', csv: '\uD83D\uDCC8',
        jpg: '\uD83D\uDDBC', jpeg: '\uD83D\uDDBC', png: '\uD83D\uDDBC', gif: '\uD83D\uDDBC', webp: '\uD83D\uDDBC',
        mp4: '\uD83C\uDFAC', avi: '\uD83C\uDFAC', mkv: '\uD83C\uDFAC',
        mp3: '\uD83C\uDFB5', wav: '\uD83C\uDFB5',
        zip: '\uD83D\uDCE6', rar: '\uD83D\uDCE6', '7z': '\uD83D\uDCE6',
        txt: '\uD83D\uDCC4', json: '\uD83D\uDCC4', xml: '\uD83D\uDCC4',
    };
    return icons[ext] || '\uD83D\uDCC1';
}

function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

async function downloadFile(id) {
    const result = await safeInvoke('download_file', { fileId: id });
    if (result) toast('Загрузка начата', 'success');
}

async function deleteFile(id, name) {
    const result = await safeInvoke('delete_file', { fileId: id });
    if (result !== null) {
        toast(`"${name}" удалён`, 'success');
        refreshFiles();
        refreshBalances();
        refreshClientStats();
    }
}

// ─── Balance ─────────────────────────────────────────────────────

async function refreshBalances() {
    const client = await safeInvoke('get_client_balance');
    if (client) {
        $('#client-balance').textContent = client.balance.toFixed(2);
        $('#client-bonus').textContent = client.bonus.toFixed(2);
    }

    const keeper = await safeInvoke('get_keeper_balance');
    if (keeper) {
        $('#keeper-balance').textContent = keeper.balance.toFixed(2);
        $('#keeper-pending').textContent = keeper.pending.toFixed(2);
        const payoutBtn = $('#btn-payout');
        payoutBtn.disabled = !keeper.can_withdraw;
        if (keeper.payout_note) {
            $('#payout-note').textContent = keeper.payout_note;
        }
    }
}

// Topup
$('#btn-topup').addEventListener('click', () => {
    $('#modal-topup').classList.remove('hidden');
    updateTopupPreview();
});

$('#btn-cancel-topup').addEventListener('click', () => {
    $('#modal-topup').classList.add('hidden');
});

$('#modal-topup .modal-overlay').addEventListener('click', () => {
    $('#modal-topup').classList.add('hidden');
});

$('#topup-amount').addEventListener('input', updateTopupPreview);

$$('.method-btn').forEach(btn => {
    btn.addEventListener('click', () => {
        $$('.method-btn').forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        state.topupMethod = btn.dataset.method;
        updateTopupPreview();
    });
});

function updateTopupPreview() {
    const amount = parseFloat($('#topup-amount').value) || 0;
    const commission = state.topupMethod === 'card' ? amount * 0.025 : 0;
    const total = amount + commission;

    $('#topup-sum').textContent = formatMoney(amount);
    $('#topup-commission').textContent = formatMoney(commission);
    $('#topup-total').textContent = formatMoney(total);
}

$('#btn-do-topup').addEventListener('click', async () => {
    const amount = parseFloat($('#topup-amount').value) || 0;
    if (amount < 10) { toast('Минимальная сумма: 10 ₽', 'error'); return; }

    const result = await safeInvoke('topup_client', {
        amount,
        method: state.topupMethod,
    });

    if (result) {
        toast(`Пополнено: ${formatMoney(result.amount)}`, 'success');
        $('#modal-topup').classList.add('hidden');
        refreshBalances();
        refreshPaymentHistory();
    }
});

// Payout
$('#btn-payout').addEventListener('click', async () => {
    const result = await safeInvoke('request_payout');
    if (result) {
        toast(result, 'success');
        refreshBalances();
        refreshPaymentHistory();
    }
});

// ─── Calculator ──────────────────────────────────────────────────

$$('.disk-btn').forEach(btn => {
    btn.addEventListener('click', () => {
        $$('.disk-btn').forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        state.calcDisk = btn.dataset.disk;
        updateCalculator();
    });
});

$('#calc-volume').addEventListener('input', updateCalculator);
$('#calc-unit').addEventListener('change', updateCalculator);

async function updateCalculator() {
    let volume = parseFloat($('#calc-volume').value) || 0;
    if ($('#calc-unit').value === 'mb') {
        volume = volume / 1024;
    }

    const result = await safeInvoke('calculate_storage_cost', {
        gb: volume,
        diskType: state.calcDisk,
    });

    if (result) {
        $('#calc-monthly').textContent = formatMoney(result.cost_per_month);
        $('#calc-5min').textContent = '~' + formatMoney(result.cost_per_5min);
        $('#cmp-hdd').textContent = formatMoney(result.comparison.hdd_monthly);
        $('#cmp-ssd').textContent = formatMoney(result.comparison.ssd_monthly);
        $('#cmp-nvme').textContent = formatMoney(result.comparison.nvme_monthly);
    }
}

// ─── Payment History ─────────────────────────────────────────────

async function refreshPaymentHistory() {
    const history = await safeInvoke('get_payment_history');
    const container = $('#payment-history');

    if (!history || history.length === 0) {
        container.innerHTML = '<div class="empty-state"><p>Операций пока нет</p></div>';
        return;
    }

    const walletLabels = { client: 'Клиент', keeper: 'Хранитель' };
    const kindLabels = {
        deposit: 'Пополнение',
        storage_fee: 'Хранение',
        payout: 'Вывод',
        referral_bonus: 'Реферал',
        relay_bonus: 'Relay',
    };

    container.innerHTML = history.slice().reverse().map(h => `
        <div class="history-item">
            <span class="history-wallet">${walletLabels[h.wallet] || h.wallet}</span>
            <span class="history-desc">${h.description}</span>
            <span class="history-amount ${h.amount >= 0 ? 'positive' : 'negative'}">
                ${h.amount >= 0 ? '+' : ''}${formatMoney(h.amount)}
            </span>
        </div>
    `).join('');
}

// ─── Keeper / Status ─────────────────────────────────────────────

$('#keeper-toggle').addEventListener('click', async (e) => {
    const enabled = e.target.checked;
    if (enabled) {
        $('#modal-agreement').classList.remove('hidden');
        e.target.checked = false;
        return;
    }
    const result = await safeInvoke('toggle_keeper_mode', { enabled: false });
    if (result !== null) {
        state.isKeeper = false;
        updateKeeperUI();
    }
});

$('#agree-check').addEventListener('change', (e) => {
    $('#btn-agree').disabled = !e.target.checked;
});

$('#btn-agree').addEventListener('click', async () => {
    const result = await safeInvoke('toggle_keeper_mode', { enabled: true });
    if (result !== null) {
        state.isKeeper = true;
        toast('Режим хранителя активирован', 'success');
        $('#modal-agreement').classList.add('hidden');
        $('#keeper-toggle').checked = true;
        updateKeeperUI();
    }
});

$('#btn-decline-agreement').addEventListener('click', () => {
    $('#modal-agreement').classList.add('hidden');
});

$('#modal-agreement .modal-overlay').addEventListener('click', () => {
    $('#modal-agreement').classList.add('hidden');
});

$('#btn-toggle-relay').addEventListener('click', async () => {
    const currentText = $('#btn-toggle-relay').textContent;
    const enable = currentText === 'Включить';
    const result = await safeInvoke('toggle_relay', { enabled: enable });
    if (result !== null) {
        toast(enable ? 'Relay включён (+3% к выплате)' : 'Relay выключен', 'success');
        refreshKeeperStats();
    }
});

async function refreshKeeperStats() {
    const stats = await safeInvoke('get_keeper_stats');
    if (!stats) return;

    state.isKeeper = stats.is_active;
    $('#keeper-toggle').checked = stats.is_active;

    $('#keeper-rating').textContent = stats.rating.toFixed(2);
    $('#keeper-storage').textContent = formatGB(stats.storage_provided_gb);
    $('#keeper-earnings').textContent = formatMoney(stats.earnings_total);
    $('#keeper-peers').textContent = stats.connected_peers;

    const pct = Math.round(stats.rating * 100);
    $('#rating-fill').style.width = pct + '%';

    if (stats.relay_enabled) {
        $('#relay-status').textContent = 'Активен';
        $('#relay-status').classList.add('green');
        $('#btn-toggle-relay').textContent = 'Выключить';
        $('#relay-bonus-row').classList.remove('hidden');
    } else {
        $('#relay-status').textContent = 'Выключен';
        $('#relay-status').classList.remove('green');
        $('#btn-toggle-relay').textContent = 'Включить';
        $('#relay-bonus-row').classList.add('hidden');
    }

    updateKeeperUI();
}

function updateKeeperUI() {
    if (state.isKeeper) {
        $('#keeper-panel').classList.remove('inactive');
        $('#keeper-inactive-msg').classList.add('hidden');
        $('#payout-note').textContent = '';
    } else {
        $('#keeper-panel').classList.add('inactive');
        $('#keeper-inactive-msg').classList.remove('hidden');
    }
}

async function refreshClientStats() {
    const stats = await safeInvoke('get_client_stats');
    if (!stats) return;

    $('#client-storage').textContent = formatGB(stats.storage_used_gb);
    $('#client-files').textContent = stats.files_count;
    $('#client-monthly-cost').textContent = formatMoney(stats.monthly_cost);
    $('#peers-count').textContent = stats.connected_peers + ' узл.';
}

// ─── Referrals ───────────────────────────────────────────────────

async function refreshReferralInfo() {
    const info = await safeInvoke('get_referral_info');
    if (!info) return;

    $('#referral-link').value = info.referral_link;
    $('#referral-code-display').textContent = info.referral_code;
    $('#referral-count').textContent = info.invited_count;
    $('#referral-earnings').textContent = formatMoney(info.total_earnings);
}

$('#btn-copy-referral').addEventListener('click', async () => {
    const link = $('#referral-link').value;
    try {
        await navigator.clipboard.writeText(link);
        toast('Ссылка скопирована', 'success');
    } catch {
        // Fallback
        const input = $('#referral-link');
        input.select();
        document.execCommand('copy');
        toast('Ссылка скопирована', 'success');
    }
});

// ─── Settings ────────────────────────────────────────────────────

async function refreshSettings() {
    const settings = await safeInvoke('get_settings');
    if (!settings) return;

    $('#setting-bootstrap').value = settings.bootstrap_nodes.join('\n');
    $('#setting-storage-limit').value = settings.storage_limit_gb;
    $('#setting-autostart').checked = settings.auto_start;
    $('#setting-russia').checked = settings.russia_only;

    const wallet = await safeInvoke('get_wallet_info');
    if (wallet) {
        $('#settings-peer-id').textContent = wallet.peer_id;
        $('#settings-mnemonic').textContent = wallet.mnemonic;
    }
}

$('#btn-show-mnemonic').addEventListener('click', () => {
    const el = $('#settings-mnemonic');
    el.classList.toggle('hidden');
    $('#btn-show-mnemonic').textContent = el.classList.contains('hidden') ? 'Показать' : 'Скрыть';
});

async function refreshGeo() {
    const geo = await safeInvoke('check_geo');
    if (!geo) return;

    const el = $('#geo-status');
    if (geo.verified) {
        el.innerHTML = '<span class="dot online"></span><span>РФ подтверждена (' + geo.ip + ')</span>';
    } else {
        el.innerHTML = '<span class="dot warning"></span><span>' + geo.country + '</span>';
    }
}

$('#btn-save-settings').addEventListener('click', async () => {
    const settings = {
        bootstrapNodes: $('#setting-bootstrap').value.split('\n').filter(s => s.trim()),
        storageLimitGb: parseInt($('#setting-storage-limit').value) || 100,
        autoStart: $('#setting-autostart').checked,
        russiaOnly: $('#setting-russia').checked,
    };

    const result = await safeInvoke('save_settings', { settings });
    if (result !== null) {
        toast('Настройки сохранены', 'success');
    }
});

// ─── Timer (5-minute calculation cycle) ───────────────────────────

function startTimer() {
    state.timerSeconds = 300;
    updateTimerDisplay();

    state.timerInterval = setInterval(() => {
        state.timerSeconds--;
        if (state.timerSeconds <= 0) {
            state.timerSeconds = 300;
            onTick();
        }
        updateTimerDisplay();
    }, 1000);
}

function updateTimerDisplay() {
    const mins = Math.floor(state.timerSeconds / 60);
    const secs = state.timerSeconds % 60;
    $('#timer-text').textContent =
        String(mins).padStart(2, '0') + ':' + String(secs).padStart(2, '0');
}

async function onTick() {
    if (!state.initialized) return;

    const result = await safeInvoke('simulate_tick');
    if (result) {
        refreshBalances();
        refreshKeeperStats();
    }
}

// ─── Periodic Refresh ────────────────────────────────────────────

setInterval(() => {
    if (!state.initialized) return;
    refreshClientStats();
    // Refresh other data less frequently
}, 15000);

// ─── Initialize ──────────────────────────────────────────────────

init();
