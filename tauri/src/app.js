// ═══════════════════════════════════════════════════════════════
// Соты — P2P Хранилище: Frontend Application Logic v3
// ═══════════════════════════════════════════════════════════════

const { invoke } = window.__TAURI__.core;
const $ = s => { try { return document.querySelector(s); } catch(e) { return null; } };
const $$ = s => { try { return document.querySelectorAll(s); } catch(e) { return []; } };

// ─── Error diagnostics ──────────────────────────────────────
window.onerror = function(msg, url, line, col, err) {
    debugLog('JS Error: ' + msg + ' at ' + line + ':' + col);
    return false;
};
window.addEventListener('unhandledrejection', function(e) {
    debugLog('Promise Error: ' + (e.reason && e.reason.message || e.reason || 'unknown'));
});

// ─── Debug log to file (survives app freeze) ───────────────
let _debugLogReady = false;
let _debugLogQueue = [];

async function debugLog(msg) {
    const ts = new Date().toLocaleTimeString();
    const text = ts + ': ' + msg;
    console.log('[DBG]', msg);
    if (!_debugLogReady) {
        _debugLogQueue.push(text);
        return;
    }
    try {
        await invoke('debug_log', { message: text });
    } catch(e) {
        console.error('[DBG] write error:', e);
    }
}

// Flush queued logs once invoke is confirmed working
async function flushDebugLog() {
    _debugLogReady = true;
    for (const text of _debugLogQueue) {
        try { await invoke('debug_log', { message: '[queued] ' + text }); } catch(e) {}
    }
    _debugLogQueue = [];
}

function fmtMoney(n) { return n.toFixed(2).replace(/\B(?=(\d{3})+(?!\d))/g, ' ') + ' \u20BD'; }
function fmtBytes(b) { if (!b) return '0 \u0411'; const u = ['\u0411','\u041A\u0411','\u041C\u0411','\u0413\u0411']; const i = Math.floor(Math.log(b)/Math.log(1024)); return (b/Math.pow(1024,i)).toFixed(i>0?1:0)+' '+u[i]; }
function fmtGB(g) { return g<0.001?'0 \u0413\u0411': g<1?(g*1024).toFixed(1)+' \u041C\u0411': g.toFixed(2)+' \u0413\u0411'; }
function esc(s) { if (typeof DOMPurify !== 'undefined') { return DOMPurify.sanitize(s); } const d=document.createElement('div'); d.textContent=s; return d.innerHTML; }

function toast(msg, type='info') {
    const c=$('#toast-container'); if(!c) return;
    const el=document.createElement('div');
    el.className=`toast ${type}`; el.textContent=msg; c.appendChild(el);
    setTimeout(()=>el.remove(), 3000);
}

async function inv(cmd, args={}) {
    const INVOKE_TIMEOUT = 5000; // 5 seconds
    let timerId;
    try {
        const timeout = new Promise((_, reject) => {
            timerId = setTimeout(() => reject(new Error('TIMEOUT ' + INVOKE_TIMEOUT + 'ms')), INVOKE_TIMEOUT);
        });
        const result = await Promise.race([
            invoke(cmd, args).then(r => { clearTimeout(timerId); return r; }),
            timeout
        ]);
        return result;
    } catch(e) {
        if (timerId) clearTimeout(timerId);
        const errMsg = String(e.message || e);
        debugLog('inv[' + cmd + '] ERROR: ' + errMsg);
        toast('[' + cmd + '] ' + errMsg, 'error');
        return null;
    }
}

const state = { initialized:false, isKeeper:false, calcDisk:'hdd', topupMethod:'card', timerSeconds:300, creditEnabled:false };

// ─── Freeze diagnostics ──────────────────────────────────
let _freezeDiagStarted = false;
function startFreezeDiagnostics() {
    if (_freezeDiagStarted) return;
    _freezeDiagStarted = true;
    debugLog('FREEZE DIAG: starting click diagnostics...');
    document.addEventListener('click', function(e) {
        debugLog('FREEZE DIAG: click at ' + Date.now() + ' on ' + (e.target?.tagName || '?') + '#' + (e.target?.id || ''));
    }, true); // capture phase
    document.addEventListener('mousedown', function() {
        debugLog('FREEZE DIAG: mousedown at ' + Date.now());
    }, true);
    document.addEventListener('mouseup', function() {
        debugLog('FREEZE DIAG: mouseup at ' + Date.now());
    }, true);
    // Test event loop every 2 seconds
    setInterval(function() {
        debugLog('FREEZE DIAG: event loop alive at ' + Date.now());
    }, 2000);
}

// ─── Onboarding ──────────────────────────────────────────────

async function init() {
    await flushDebugLog();
    debugLog('init() called');
    const isInit = await inv('check_initialized');
    debugLog('check_initialized returned: ' + isInit);
    if (isInit) showApp();
    startTimer();
}

$('#btn-create-wallet')?.addEventListener('click', async () => {
    const info = await inv('create_wallet');
    if (info) { 
        const mt = $('#mnemonic-text'); if(mt) mt.textContent = info.mnemonic;
        const pid = $('#peer-id-display'); if(pid) pid.textContent = info.peerId;
        $('#wallet-created')?.classList.remove('hidden'); 
        $('#onboard-actions')?.classList.add('hidden'); 
    }
});

$('#btn-restore-wallet')?.addEventListener('click', () => { 
    $('#restore-form')?.classList.remove('hidden'); 
    $('#onboard-actions')?.classList.add('hidden'); 
});
$('#btn-cancel-restore')?.addEventListener('click', () => { 
    $('#restore-form')?.classList.add('hidden'); 
    $('#onboard-actions')?.classList.remove('hidden'); 
});
$('#btn-do-restore')?.addEventListener('click', async () => {
    const m = $('#mnemonic-input')?.value?.trim();
    if (!m) { toast('Введите фразу','error'); return; }
    const info = await inv('restore_wallet', { mnemonic: m });
    if (info) { 
        const mt = $('#mnemonic-text'); if(mt) mt.textContent = info.mnemonic;
        const pid = $('#peer-id-display'); if(pid) pid.textContent = info.peerId;
        $('#wallet-created')?.classList.remove('hidden'); 
        $('#restore-form')?.classList.add('hidden'); 
    }
});

// ─── Copy buttons (clipboard) ───────────────────────────────

async function copyToClipboard(text, btnEl) {
    try {
        await navigator.clipboard.writeText(text);
        if (btnEl) btnEl.classList.add('copied');
        if (btnEl) btnEl.textContent = '\u2705';
        toast('Скопировано', 'success');
        setTimeout(() => { 
            if (btnEl) { btnEl.classList.remove('copied'); btnEl.innerHTML = '&#128203;'; } 
        }, 2000);
    } catch(e) {
        toast('Не удалось скопировать', 'error');
    }
}

$('#btn-copy-mnemonic')?.addEventListener('click', () => {
    copyToClipboard($('#mnemonic-text')?.textContent?.trim(), $('#btn-copy-mnemonic'));
});

$('#btn-copy-peer-id')?.addEventListener('click', () => {
    copyToClipboard($('#peer-id-display')?.textContent?.trim(), $('#btn-copy-peer-id'));
});

// ─── Key written + Continue buttons ─────────────────────────

$('#btn-key-written')?.addEventListener('click', () => {
    const btn = $('#btn-continue');
    if (btn) { btn.disabled = false; btn.classList.add('active'); }
    const keyBtn = $('#btn-key-written');
    if (keyBtn) { keyBtn.disabled = true; keyBtn.style.opacity = '0.4'; keyBtn.style.cursor = 'default'; }
    toast('Нажмите "Продолжить" для входа', 'info');
});

$('#btn-continue')?.addEventListener('click', async () => {
    const continueBtn = $('#btn-continue');
    if (continueBtn?.disabled) return;
    debugLog('btn-continue clicked, preparing verification');
    // Show mnemonic verification modal with 3 random words
    const mnemonic = $('#mnemonic-text')?.textContent?.trim() || '';
    const words = mnemonic.split(/\s+/);
    const positions = [
        Math.floor(Math.random() * 24),
        Math.floor(Math.random() * 24),
        Math.floor(Math.random() * 24)
    ];
    // Deduplicate positions
    const uniquePositions = [...new Set(positions)];
    while (uniquePositions.length < 3) {
        uniquePositions.push(Math.floor(Math.random() * 24));
    }
    uniquePositions.forEach((pos, i) => {
        const word = words[pos] || '?';
        const hint = $(`#verify-word-${i+1}-hint`); if(hint) hint.textContent = word;
        const label = $(`#verify-label-${i+1}`); if(label) label.textContent = `Слово #${i+1} (позиция ${pos+1})`;
        const input = $(`#verify-word-${i+1}`); if(input) input.value = '';
    });
    $('#verify-error')?.classList.add('hidden');
    $('#mnemonic-verify')?.classList.remove('hidden');
    // Hide the key-display and buttons behind it
    // FIX: NodeList[0] instead of NodeList.first() (not available in Chromium!)
    const btns = $$('.onboarding-btns');
    if (btns.length > 0) btns[0].classList.add('hidden');
    // Store positions for verification
    window._verifyPositions = uniquePositions;
    debugLog('Verification modal shown, positions: ' + JSON.stringify(uniquePositions));
});

$('#btn-verify-mnemonic')?.addEventListener('click', async () => {
    debugLog('btn-verify-mnemonic clicked');
    const positions = window._verifyPositions || [0, 1, 2];
    const expected = [
        $('#verify-word-1')?.value?.trim() || '',
        $('#verify-word-2')?.value?.trim() || '',
        $('#verify-word-3')?.value?.trim() || ''
    ];
    debugLog('Verifying positions=' + JSON.stringify(positions) + ' expected=' + JSON.stringify(expected));
    const ok = await inv('verify_mnemonic_words_cmd', { positions, expected });
    debugLog('verify_mnemonic_words_cmd returned: ' + ok);
    if (ok) {
        debugLog('Mnemonic verified! Calling confirm_mnemonic_shown...');
        await inv('confirm_mnemonic_shown');
        debugLog('confirm_mnemonic_shown done, calling showApp()');
        showApp();
    } else {
        $('#verify-error')?.classList.remove('hidden');
        toast('Неверные слова', 'error');
    }
});

$('#btn-verify-cancel')?.addEventListener('click', () => {
    $('#mnemonic-verify')?.classList.add('hidden');
    // FIX: NodeList[0] instead of NodeList.first()
    const btns = $$('.onboarding-btns');
    if (btns.length > 0) btns[0].classList.remove('hidden');
    window._verifyPositions = null;
});

function showApp() {
    debugLog('showApp() called at ' + Date.now());
    $('#onboarding')?.classList.add('hidden');
    debugLog('showApp: onboarding hidden');
    $('#app-main')?.classList.remove('hidden');
    debugLog('showApp: app-main shown');
    state.initialized = true;
    debugLog('showApp: state.initialized = true');
    startFreezeDiagnostics();
    debugLog('showApp: freeze diagnostics started');
    setTimeout(() => {
        debugLog('refreshAll starting at ' + Date.now());
        refreshAll().then(() => {
            debugLog('refreshAll completed at ' + Date.now());
        }).catch(e => {
            debugLog('refreshAll ERROR: ' + (e.message || e));
        });
    }, 500);
    debugLog('showApp() finished synchronously at ' + Date.now());
}

// ─── Tab Navigation ──────────────────────────────────────────

// Tab switching helper (shared by top-nav and bottom-nav)
function switchTab(tabName) {
    debugLog('Tab switched: ' + tabName);
    // Update top nav (desktop)
    $$('.nav-btn').forEach(b => { b.classList.toggle('active', b.dataset?.tab === tabName); });
    // Update bottom nav (mobile)
    $$('.bottom-nav-btn').forEach(b => { b.classList.toggle('active', b.dataset?.tab === tabName); });
    // Switch tab panels
    $$('.tab').forEach(t => t.classList.remove('active'));
    $(`#tab-${tabName}`)?.classList.add('active');
    // Scroll to top on mobile
    window.scrollTo({ top: 0, behavior: 'smooth' });
}

$$('.nav-btn').forEach(btn => btn.addEventListener('click', () => switchTab(btn.dataset?.tab)));
$$('.bottom-nav-btn').forEach(btn => btn.addEventListener('click', () => switchTab(btn.dataset?.tab)));

// ─── Refresh ─────────────────────────────────────────────────

async function refreshAll() {
    const steps = [
        ['refreshBalances', () => refreshBalances()],
        ['refreshFiles', () => refreshFiles()],
        ['refreshKeeperStats', () => refreshKeeperStats()],
        ['refreshClientStats', () => refreshClientStats()],
        ['refreshPaymentHistory', () => refreshPaymentHistory()],
        ['refreshReferralInfo', () => refreshReferralInfo()],
        ['refreshSettings', () => refreshSettings()],
        ['updateCalculator', () => updateCalculator()],
    ];
    for (const [name, fn] of steps) {
        try {
            debugLog('>> ' + name + ' at ' + Date.now() + '...');
            await fn();
            debugLog('<< ' + name + ' OK at ' + Date.now());
        } catch(e) {
            debugLog('!! ' + name + ' ERROR: ' + (e.message || e));
        }
    }
    debugLog('refreshAll complete at ' + Date.now());
}

// ─── Files ───────────────────────────────────────────────────

const dropZone = $('#drop-zone'), fileInput = $('#file-input');
$('#upload-btn')?.addEventListener('click', () => { if(fileInput) fileInput.click(); });
if (dropZone) {
    dropZone.addEventListener('dragover', e => { e.preventDefault(); dropZone.classList.add('dragover'); });
    dropZone.addEventListener('dragleave', () => dropZone.classList.remove('dragover'));
    dropZone.addEventListener('drop', e => { e.preventDefault(); dropZone.classList.remove('dragover'); handleFiles(e.dataTransfer?.files); });
}
if (fileInput) {
    fileInput.addEventListener('change', () => { handleFiles(fileInput.files); fileInput.value = ''; });
}

async function handleFiles(fileList) {
    const dt = $('#disk-type-select')?.value || 'hdd';
    if (!fileList) return;
    for (const f of fileList) {
        const r = await inv('upload_file', { name: f.name, sizeBytes: f.size, diskType: dt });
        if (r) toast(`"${f.name}" загружен`, 'success');
        else toast(`"${f.name}" — ошибка загрузки`, 'error');
    }
    await Promise.all([refreshFiles(), refreshBalances(), refreshClientStats(), refreshPaymentHistory()]);
}

async function refreshFiles() {
    const files = await inv('get_files');
    const c = $('#file-list'); if(!c) return;
    if (!files || !files.length) { c.innerHTML = '<div class="empty-state"><p>Файлы отсутствуют</p></div>'; return; }
    c.innerHTML = files.map(f => `
        <div class="file-item" data-id="${f.id}">
            <div class="file-icon">${getFileIcon(f.name)}</div>
            <div class="file-info">
                <div class="file-name">${esc(f.name)}</div>
                <div class="file-meta"><span>${fmtBytes(f.sizeBytes)}</span><span>${f.diskType?.toUpperCase()}</span><span>${f.isCredit?'[кредит] ':''}${fmtMoney(f.costPerMonth)}/мес</span></div>
            </div>
            <div class="replica-badges">${'<span class="replica-dot"></span>'.repeat(Math.min(f.replicas || 0, 4))}</div>
            <div class="file-cost">${fmtMoney(f.costPerMonth)}/мес</div>
            <div class="file-actions">
                <button class="btn btn-sm btn-outline" onclick="dlFile('${f.id}')">Скачать</button>
                <button class="btn btn-sm btn-danger" onclick="delFile('${f.id}','${esc(f.name)}')">Удалить</button>
            </div>
        </div>`).join('');
}

function getFileIcon(n) { const x = (n || '').split('.').pop().toLowerCase(); return {pdf:'\uD83D\uDCC4',doc:'\uD83D\uDCC3',docx:'\uD83D\uDCC3',xls:'\uD83D\uDCC8',xlsx:'\uD83D\uDCC8',jpg:'\uD83D\uDDBC',jpeg:'\uD83D\uDDBC',png:'\uD83D\uDDBC',mp4:'\uD83C\uDFAC',mp3:'\uD83C\uDFB5',zip:'\uD83D\uDCE6'}[x] || '\uD83D\uDCC1'; }

async function dlFile(id) { const r = await inv('download_file', {fileId: id}); if (r) toast('Загрузка начата', 'success'); }
async function delFile(id, name) { const r = await inv('delete_file', {fileId: id}); if (r !== null) { toast(`"${name}" удалён`, 'success'); await Promise.all([refreshFiles(), refreshBalances(), refreshClientStats()]); } }

// ─── Balance ─────────────────────────────────────────────────

async function refreshBalances() {
    const c = await inv('get_client_balance');
    if (c) {
        const el = (id) => document.getElementById(id);
        if (el('client-balance')) el('client-balance').textContent = c.balance.toFixed(2);
        if (el('client-bonus')) el('client-bonus').textContent = c.bonus.toFixed(2);
        state.creditEnabled = c.creditStorageEnabled;
        if (el('credit-mult-label')) el('credit-mult-label').textContent = c.creditStorageEnabled ? 'x1.5' : 'x1.0';

        // Low balance warning
        const wb = el('low-balance-badge');
        if (wb) { if (c.lowBalanceWarning) wb.classList.remove('hidden'); else wb.classList.add('hidden'); }

        // Credit period
        const cb = el('credit-period-badge');
        const cuw = el('credit-upload-warning');
        if (c.creditAction === 'block_uploads') {
            if (cb) cb.classList.remove('hidden');
            const h = Math.floor((c.creditTicksRemaining || 0) / 12);
            const m = (c.creditTicksRemaining || 0) % 12;
            const ct = el('credit-timer');
            if (ct) ct.textContent = `${h}:${String(m * 5).padStart(2, '0')}`;
            if (cuw) { cuw.classList.remove('hidden'); cuw.textContent = 'Загрузки заблокированы: нулевой баланс. Пополните в течение кредитного периода.'; }
        } else if (c.creditAction === 'delete_data') {
            if (cuw) { cuw.textContent = 'Кредитный период истёк. Данные удалены.'; cuw.classList.remove('hidden'); }
            if (cb) cb.classList.add('hidden');
        } else {
            if (cb) cb.classList.add('hidden');
            if (cuw) cuw.classList.add('hidden');
        }
    }
    const k = await inv('get_keeper_balance');
    if (k) {
        const el = (id) => document.getElementById(id);
        if (el('keeper-balance')) el('keeper-balance').textContent = k.balance.toFixed(2);
        if (el('keeper-pending')) el('keeper-pending').textContent = k.pending.toFixed(2);
        const pb = el('btn-payout');
        if (pb) pb.disabled = !k.canWithdraw;
        if (k.payoutNote) { const pn = el('payout-note'); if (pn) pn.textContent = k.payoutNote; }
    }
}

$('#btn-topup')?.addEventListener('click', () => $('#modal-topup')?.classList.remove('hidden'));
$('#btn-cancel-topup')?.addEventListener('click', () => $('#modal-topup')?.classList.add('hidden'));
$('#modal-topup .modal-overlay')?.addEventListener('click', () => $('#modal-topup')?.classList.add('hidden'));
$('#topup-amount')?.addEventListener('input', updateTopup);
$$('.method-btn').forEach(b => b.addEventListener('click', () => {
    $$('.method-btn').forEach(x => x.classList.remove('active'));
    b.classList.add('active');
    state.topupMethod = b.dataset?.method || 'card';
    updateTopup();
}));

function updateTopup() {
    const a = parseFloat($('#topup-amount')?.value) || 0;
    const comm = state.topupMethod === 'card' ? a * 0.025 : 0;
    const el = (id) => document.getElementById(id);
    if (el('topup-sum')) el('topup-sum').textContent = fmtMoney(a);
    if (el('topup-commission')) el('topup-commission').textContent = fmtMoney(comm);
    if (el('topup-total')) el('topup-total').textContent = fmtMoney(a + comm);
}

$('#btn-do-topup')?.addEventListener('click', async () => {
    const a = parseFloat($('#topup-amount')?.value) || 0;
    if (a < 10) { toast('Минимум 10 \u20BD', 'error'); return; }
    const r = await inv('topup_client', {amount: a, method: state.topupMethod});
    if (r) {
        toast(`Пополнено: ${fmtMoney(r.amount)}`, 'success');
        $('#modal-topup')?.classList.add('hidden');
        await Promise.all([refreshBalances(), refreshPaymentHistory()]);
    }
});

$('#btn-payout')?.addEventListener('click', async () => {
    const r = await inv('request_payout');
    if (r) { toast(r, 'success'); await Promise.all([refreshBalances(), refreshPaymentHistory()]); }
});

// ─── Calculator ─────────────────────────────────────────────

$$('.disk-btn').forEach(b => b.addEventListener('click', () => {
    $$('.disk-btn').forEach(x => x.classList.remove('active'));
    b.classList.add('active');
    state.calcDisk = b.dataset?.disk || 'hdd';
    updateCalculator();
}));
$('#calc-volume')?.addEventListener('input', updateCalculator);
$('#calc-unit')?.addEventListener('change', updateCalculator);
$('#calc-credit-toggle')?.addEventListener('change', updateCalculator);

async function updateCalculator() {
    let vol = parseFloat($('#calc-volume')?.value) || 0;
    if (($('#calc-unit')?.value) === 'mb') vol /= 1024;
    const credit = $('#calc-credit-toggle')?.checked || false;
    const r = await inv('calculate_storage_cost', {gb: vol, diskType: state.calcDisk, credit});
    if (r) {
        const el = (id) => document.getElementById(id);
        if (el('calc-monthly')) el('calc-monthly').textContent = fmtMoney(r.costPerMonth);
        if (el('calc-5min')) el('calc-5min').textContent = '~' + fmtMoney(r.costPer5min);
        if (el('calc-repl-mult')) el('calc-repl-mult').textContent = `x4 / x${r.creditMultiplier.toFixed(1)}`;
        if (el('cmp-hdd')) el('cmp-hdd').textContent = fmtMoney(r.comparison.hddMonthly);
        if (el('cmp-ssd')) el('cmp-ssd').textContent = fmtMoney(r.comparison.ssdMonthly);
        if (el('cmp-nvme')) el('cmp-nvme').textContent = fmtMoney(r.comparison.nvmeMonthly);
    }
}

// ─── History ─────────────────────────────────────────────────

async function refreshPaymentHistory() {
    const h = await inv('get_payment_history'), c = $('#payment-history');
    if (!c) return;
    if (!h || !h.length) { c.innerHTML = '<div class="empty-state"><p>Операций пока нет</p></div>'; return; }
    const wl = {client: 'Клиент', keeper: 'Хранитель'};
    c.innerHTML = h.slice().reverse().map(x => `<div class="history-item"><span class="history-wallet">${wl[x.wallet] || x.wallet}</span><span class="history-desc">${esc(x.description)}</span><span class="history-amount ${x.amount >= 0 ? 'positive' : 'negative'}">${x.amount >= 0 ? '+' : ''}${fmtMoney(x.amount)}</span></div>`).join('');
}

// ─── Keeper / Status ────────────────────────────────────────

$('#keeper-toggle')?.addEventListener('click', async e => {
    if (e.target.checked) { $('#modal-agreement')?.classList.remove('hidden'); e.target.checked = false; return; }
    const r = await inv('toggle_keeper_mode', {enabled: false});
    if (r !== null) { state.isKeeper = false; updateKeeperUI(); }
});

$('#agree-check')?.addEventListener('change', e => { const ab = $('#btn-agree'); if (ab) ab.disabled = !e.target.checked; });
$('#btn-agree')?.addEventListener('click', async () => {
    const r = await inv('toggle_keeper_mode', {enabled: true});
    if (r !== null) {
        state.isKeeper = true;
        toast('Режим хранителя активирован', 'success');
        $('#modal-agreement')?.classList.add('hidden');
        const kt = $('#keeper-toggle'); if (kt) kt.checked = true;
        updateKeeperUI();
    }
});
$('#btn-decline-agreement')?.addEventListener('click', () => $('#modal-agreement')?.classList.add('hidden'));
$('#modal-agreement .modal-overlay')?.addEventListener('click', () => $('#modal-agreement')?.classList.add('hidden'));

$('#btn-toggle-relay')?.addEventListener('click', async () => {
    const cur = $('#btn-toggle-relay')?.textContent;
    const enable = cur === 'Включить';
    const r = await inv('toggle_relay', {enabled: enable});
    if (r !== null) toast(enable ? 'Relay включён (+2%)' : 'Relay выключен', 'success');
    refreshKeeperStats();
});

$('#btn-graceful-shutdown')?.addEventListener('click', async () => {
    if (!confirm('Graceful shutdown уведомит сеть. Продолжить?')) return;
    const r = await inv('initiate_shutdown');
    if (r) toast(r, 'success');
});

$('#btn-view-agency')?.addEventListener('click', async () => {
    const text = await inv('get_legal_agency');
    if (text) { 
        const lt = $('#legal-title'); if(lt) lt.textContent = 'Агентский договор'; 
        const lc = $('#legal-text-content'); if(lc) lc.textContent = text; 
        $('#modal-legal')?.classList.remove('hidden'); 
    }
});
$('#btn-view-offer')?.addEventListener('click', async () => {
    const text = await inv('get_legal_offer');
    if (text) { 
        const lt = $('#legal-title'); if(lt) lt.textContent = 'Публичная оферта'; 
        const lc = $('#legal-text-content'); if(lc) lc.textContent = text; 
        $('#modal-legal')?.classList.remove('hidden'); 
    }
});
$('#btn-close-legal')?.addEventListener('click', () => $('#modal-legal')?.classList.add('hidden'));
$('#modal-legal .modal-overlay')?.addEventListener('click', () => $('#modal-legal')?.classList.add('hidden'));

async function refreshKeeperStats() {
    const s = await inv('get_keeper_stats');
    if (!s) return;
    const el = (id) => document.getElementById(id);
    state.isKeeper = s.isActive;
    const kt = $('#keeper-toggle'); if (kt) kt.checked = s.isActive;
    if (el('keeper-rating-pay')) el('keeper-rating-pay').textContent = s.ratingPay.toFixed(3);
    if (el('keeper-rating-alloc')) el('keeper-rating-alloc').textContent = s.ratingAlloc.toFixed(3);
    if (el('keeper-storage')) el('keeper-storage').textContent = fmtGB(s.storageProvidedGb);
    if (el('keeper-earnings')) el('keeper-earnings').textContent = fmtMoney(s.earningsTotal);
    if (el('keeper-peers')) el('keeper-peers').textContent = s.connectedPeers;
    if (el('white-ip-status')) el('white-ip-status').textContent = s.hasWhiteIp ? 'Да' : 'Нет (серый IP)';
    if (el('bootstrap-status')) el('bootstrap-status').textContent = s.isBootstrap ? 'Да' : 'Нет';

    const pctPay = Math.round(s.ratingPay * 100);
    const rfp = el('rating-fill-pay');
    if (rfp) rfp.style.width = pctPay + '%';

    if (s.relayEnabled) {
        if (el('relay-status')) { el('relay-status').textContent = 'Активен'; el('relay-status').classList.add('green'); }
        const btr = $('#btn-toggle-relay'); if(btr) btr.textContent = 'Выключить';
        const rbr = el('relay-bonus-row'); if(rbr) rbr.classList.remove('hidden');
    } else {
        if (el('relay-status')) { el('relay-status').textContent = s.relayBanned ? 'Заблокирован' : 'Выключен'; el('relay-status').classList.remove('green'); }
        const btr = $('#btn-toggle-relay'); if(btr) btr.textContent = 'Включить';
        const rbr = el('relay-bonus-row'); if(rbr) rbr.classList.add('hidden');
    }

    if (s.bootstrapNote) {
        const bbr = el('bootstrap-bonus-row'); if(bbr) bbr.classList.remove('hidden');
        const bbg = bbr?.querySelector('.green'); if(bbg) bbg.textContent = '+1% к выплате';
    } else {
        const bbr = el('bootstrap-bonus-row'); if(bbr) bbr.classList.add('hidden');
    }

    if (el('relay-gray-clients')) el('relay-gray-clients').textContent = s.relayGrayClients || 0;
    if (el('relay-fail-pct')) el('relay-fail-pct').textContent = s.relayFailPct ? (s.relayFailPct * 100).toFixed(1) + '%' : '0%';
    if (s.relayEnabled || (s.relayGrayClients || 0) > 0 || (s.relayFailPct || 0) > 0) {
        if (el('relay-details-row')) el('relay-details-row').classList.remove('hidden');
        if (el('relay-fail-row')) el('relay-fail-row').classList.remove('hidden');
    } else {
        if (el('relay-details-row')) el('relay-details-row').classList.add('hidden');
        if (el('relay-fail-row')) el('relay-fail-row').classList.add('hidden');
    }

    if (el('credit-keeper-status')) el('credit-keeper-status').textContent = s.creditStorageKeeper ? 'Включено' : 'Выключено';

    // Warnings panel
    const wp = el('warnings-panel');
    if (s.warnings && s.warnings.length) {
        if (wp) { wp.classList.remove('hidden'); wp.innerHTML = s.warnings.map(w => `<div class="warning-item"><span>${esc(w)}</span></div>`).join(''); }
    } else {
        if (wp) wp.classList.add('hidden');
    }

    updateKeeperUI();
}

function updateKeeperUI() {
    const el = (id) => document.getElementById(id);
    if (state.isKeeper) {
        if (el('keeper-panel')) el('keeper-panel').classList.remove('inactive');
        if (el('keeper-inactive-msg')) el('keeper-inactive-msg').classList.add('hidden');
    } else {
        if (el('keeper-panel')) el('keeper-panel').classList.add('inactive');
        if (el('keeper-inactive-msg')) el('keeper-inactive-msg').classList.remove('hidden');
    }
}

async function refreshClientStats() {
    const s = await inv('get_client_stats');
    if (!s) return;
    const el = (id) => document.getElementById(id);
    if (el('client-storage')) el('client-storage').textContent = fmtGB(s.storageUsedGb);
    if (el('client-files')) el('client-files').textContent = s.filesCount;
    if (el('client-monthly-cost')) el('client-monthly-cost').textContent = fmtMoney(s.monthlyCost);
    if (el('peers-count')) el('peers-count').textContent = (s.connectedPeers || 0) + ' узл.';
    if (el('credit-client-status')) el('credit-client-status').textContent = s.creditStorageEnabled ? 'Включено' : 'Выключено';
}

// ─── Referrals ──────────────────────────────────────────────

async function refreshReferralInfo() {
    const i = await inv('get_referral_info');
    if (!i) return;
    const el = (id) => document.getElementById(id);
    if (el('referral-link')) el('referral-link').value = i.referralLink;
    if (el('referral-code-display')) el('referral-code-display').textContent = i.referralCode;
    if (el('referral-count')) el('referral-count').textContent = i.invitedCount;
    if (el('referral-earnings')) el('referral-earnings').textContent = fmtMoney(i.totalEarnings);
}
$('#btn-copy-referral')?.addEventListener('click', async () => {
    try { await navigator.clipboard.writeText($('#referral-link')?.value || ''); toast('Скопировано', 'success'); }
    catch { const i = $('#referral-link'); if (i) { i.select(); document.execCommand('copy'); toast('Скопировано', 'success'); } }
});

// ─── Settings ───────────────────────────────────────────────

async function refreshSettings() {
    const s = await inv('get_settings');
    if (!s) return;
    const el = (id) => document.getElementById(id);
    if (el('setting-bootstrap')) el('setting-bootstrap').value = (s.bootstrapNodes || []).join('\n');
    if (el('setting-storage-limit')) el('setting-storage-limit').value = s.storageLimitGb;
    if (el('setting-autostart')) el('setting-autostart').checked = s.autoStart;
    if (el('setting-russia')) el('setting-russia').checked = s.russiaOnly;
    if (el('setting-credit-client')) el('setting-credit-client').checked = s.creditStorageClient;
    if (el('setting-credit-keeper')) el('setting-credit-keeper').checked = s.creditStorageKeeper;

    const w = await inv('get_wallet_info');
    if (w) {
        if (el('settings-peer-id')) el('settings-peer-id').textContent = w.peerId;
        const sm = el('settings-mnemonic');
        if (sm) sm.textContent = w.mnemonic || '(зашифрована)';
    }

    const g = await inv('check_geo');
    if (g) {
        if (el('settings-install-id')) el('settings-install-id').textContent = g.installationId;
        if (g.verified) {
            const gs = el('geo-status');
            if (gs) gs.innerHTML = '<span class="dot online"></span><span>РФ подтверждена</span>';
        }
    }

    const s2 = await inv('get_settings');
    if (s2) {
        if (el('setting-notify-enabled')) el('setting-notify-enabled').checked = s2.notifyEnabled;
        if (el('setting-notify-email')) el('setting-notify-email').value = s2.notifyEmail || '';
        if (el('setting-smtp-host')) el('setting-smtp-host').value = s2.smtpHost || '';
        if (el('setting-smtp-port')) el('setting-smtp-port').value = s2.smtpPort || 587;
        if (el('setting-smtp-login')) el('setting-smtp-login').value = s2.smtpLogin || '';
    }
}

$('#btn-show-mnemonic')?.addEventListener('click', () => {
    const el = $('#settings-mnemonic');
    if (el) el.classList.toggle('hidden');
    const btn = $('#btn-show-mnemonic');
    if (btn) btn.textContent = el?.classList.contains('hidden') ? 'Показать' : 'Скрыть';
});

$('#setting-credit-client')?.addEventListener('change', async e => {
    const r = await inv('toggle_credit_storage_client', {enabled: e.target.checked});
    if (r !== null) toast(r ? 'Хранение в долг включено (x1.5)' : 'Хранение в долг выключено', 'info');
    refreshSettings();
});

$('#setting-credit-keeper')?.addEventListener('change', async e => {
    const r = await inv('toggle_credit_storage_keeper', {enabled: e.target.checked});
    if (r !== null) toast(r ? 'Кредитное хранение включено' : 'Кредитное хранение выключено', 'info');
    refreshSettings();
});


$('#btn-save-settings')?.addEventListener('click', async () => {
    const el = (id) => document.getElementById(id);
    const s = {
        bootstrapNodes: el('setting-bootstrap')?.value?.split('\n').filter(x => x.trim()) || [],
        storageLimitGb: parseInt(el('setting-storage-limit')?.value) || 100,
        autoStart: el('setting-autostart')?.checked || false,
        russiaOnly: el('setting-russia')?.checked || false,
        creditStorageClient: el('setting-credit-client')?.checked || false,
        creditStorageKeeper: el('setting-credit-keeper')?.checked || false,
        notifyEnabled: el('setting-notify-enabled')?.checked || false,
        notifyEmail: el('setting-notify-email')?.value || '',
        smtpHost: el('setting-smtp-host')?.value || '',
        smtpPort: parseInt(el('setting-smtp-port')?.value) || 587,
        smtpLogin: el('setting-smtp-login')?.value || '',
        smtpPasswordEncrypted: el('setting-smtp-password')?.value || '',
        notifyLowBalance: true,
        notifyPenalty: true,
        notifyDataDelete: true,
        notifyShutdown: true,
    };
    const r = await inv('save_settings', {settings: s});
    if (r !== null) toast('Настройки сохранены', 'success');
});

// ─── Timer ───────────────────────────────────────────────────

function startTimer() {
    state.timerSeconds = 300;
    updateTimerDisplay();
    setInterval(() => {
        state.timerSeconds--;
        if (state.timerSeconds <= 0) { state.timerSeconds = 300; onTick(); }
        updateTimerDisplay();
    }, 1000);
}

function updateTimerDisplay() {
    const m = Math.floor(state.timerSeconds / 60), s = state.timerSeconds % 60;
    const tt = $('#timer-text');
    if (tt) tt.textContent = String(m).padStart(2, '0') + ':' + String(s).padStart(2, '0');
}

async function onTick() {
    if (!state.initialized) return;
    try {
        debugLog('onTick: simulate_tick...');
        const r = await inv('simulate_tick');
        if (r) {
            debugLog('onTick: refreshing balances...');
            await refreshBalances();
            await refreshKeeperStats();
            debugLog('onTick: done');
        }
    } catch(e) {
        debugLog('onTick ERROR: ' + (e.message || e));
    }
}

// ─── Periodic Refresh ───────────────────────────────────────

setInterval(() => { if (state.initialized) refreshClientStats(); }, 15000);

// ─── Agreement text loader ──────────────────────────────────

(async () => {
    try {
        debugLog('Loading agreement text...');
        const text = await inv('get_legal_agency');
        if (text) {
            const atc = $('#agreement-text-content');
            if (atc) atc.textContent = text;
        }
        debugLog('Agreement text loaded: ' + (text ? 'OK' : 'empty'));
    } catch(e) {
        debugLog('Agreement text load ERROR: ' + (e.message || e));
    }
})();

// ─── Close account ────────────────────────────────────────

$('#btn-close-account')?.addEventListener('click', async () => {
    if (!confirm('Вы уверены? Все файлы будут удалены, бонусы сгорят. Остаток средств вернётся на вашу карту.')) return;
    const r = await inv('close_client_account');
    if (r) {
        toast(`Аккаунт закрыт. Возврат: ${r.refundAmount.toFixed(2)} \u20BD.`, 'success');
        $('#app-main')?.classList.add('hidden');
        $('#onboarding')?.classList.remove('hidden');
        state.initialized = false;
    }
});

// ─── Init ────────────────────────────────────────────────────

debugLog('app.js loaded, initializing...');
init();

// ─── Footer links ────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', function() {
    const footerOffer = document.getElementById('footer-offer');
    const footerAgency = document.getElementById('footer-agency');
    if (footerOffer) {
        footerOffer.addEventListener('click', async function(e) {
            e.preventDefault();
            const text = await inv('get_legal_offer');
            if (text) {
                document.getElementById('legal-title').textContent = 'Публичная оферта';
                document.getElementById('legal-text-content').textContent = text;
                document.getElementById('modal-legal').classList.remove('hidden');
            }
        });
    }
    if (footerAgency) {
        footerAgency.addEventListener('click', async function(e) {
            e.preventDefault();
            const text = await inv('get_legal_agency');
            if (text) {
                document.getElementById('legal-title').textContent = 'Агентский договор';
                document.getElementById('legal-text-content').textContent = text;
                document.getElementById('modal-legal').classList.remove('hidden');
            }
        });
    }
    debugLog('Footer links initialized');
});
