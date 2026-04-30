// ═══════════════════════════════════════════════════════════════
// Соты — P2P Хранилище: Frontend Application Logic v2
// ═══════════════════════════════════════════════════════════════

const { invoke } = window.__TAURI__.core;
const $ = s => document.querySelector(s);
const $$ = s => document.querySelectorAll(s);

function fmtMoney(n) { return n.toFixed(2).replace(/\B(?=(\d{3})+(?!\d))/g, ' ') + ' \u20BD'; }
function fmtBytes(b) { if (!b) return '0 \u0411'; const u = ['\u0411','\u041A\u0411','\u041C\u0411','\u0413\u0411']; const i = Math.floor(Math.log(b)/Math.log(1024)); return (b/Math.pow(1024,i)).toFixed(i>0?1:0)+' '+u[i]; }
function fmtGB(g) { return g<0.001?'0 \u0413\u0411': g<1?(g*1024).toFixed(1)+' \u041C\u0411': g.toFixed(2)+' \u0413\u0411'; }
function esc(s) { const d=document.createElement('div'); d.textContent=s; return d.innerHTML; }

function toast(msg, type='info') {
    const c=$('#toast-container'), el=document.createElement('div');
    el.className=`toast ${type}`; el.textContent=msg; c.appendChild(el);
    setTimeout(()=>el.remove(), 3000);
}

async function inv(cmd, args={}) { try { return await invoke(cmd, args); } catch(e) { console.error(`[${cmd}]`,e); toast(String(e),'error'); return null; } }

const state = { initialized:false, isKeeper:false, calcDisk:'hdd', topupMethod:'card', timerSeconds:300, creditEnabled:false };

// ─── Onboarding ──────────────────────────────────────────────

async function init() {
    if (await inv('check_initialized')) showApp();
    startTimer();
}

$('#btn-create-wallet').addEventListener('click', async () => {
    const info = await inv('create_wallet');
    if (info) { $('#mnemonic-display').textContent=info.mnemonic; $('#peer-id-display').textContent=info.peer_id; $('#wallet-created').classList.remove('hidden'); $('#onboard-actions').classList.add('hidden'); }
});

$('#btn-restore-wallet').addEventListener('click', () => { $('#restore-form').classList.remove('hidden'); $('#onboard-actions').classList.add('hidden'); });
$('#btn-cancel-restore').addEventListener('click', () => { $('#restore-form').classList.add('hidden'); $('#onboard-actions').classList.remove('hidden'); });
$('#btn-do-restore').addEventListener('click', async () => {
    const m=$('#mnemonic-input').value.trim();
    if (!m) { toast('Введите фразу','error'); return; }
    const info = await inv('restore_wallet', { mnemonic: m });
    if (info) { $('#mnemonic-display').textContent=info.mnemonic; $('#peer-id-display').textContent=info.peer_id; $('#wallet-created').classList.remove('hidden'); $('#restore-form').classList.add('hidden'); }
});

$('#btn-enter-app').addEventListener('click', async () => {
    await inv('confirm_mnemonic_shown');
    showApp();
});

function showApp() {
    $('#onboarding').classList.add('hidden');
    $('#app-main').classList.remove('hidden');
    state.initialized = true;
    refreshAll();
}

// ─── Tab Navigation ──────────────────────────────────────────

$$('.nav-btn').forEach(btn => btn.addEventListener('click', () => {
    $$('.nav-btn').forEach(b=>b.classList.remove('active'));
    $$('.tab').forEach(t=>t.classList.remove('active'));
    btn.classList.add('active');
    $(`#tab-${btn.dataset.tab}`).classList.add('active');
}));

// ─── Refresh ─────────────────────────────────────────────────

async function refreshAll() {
    await Promise.all([refreshBalances(), refreshFiles(), refreshKeeperStats(), refreshClientStats(), refreshPaymentHistory(), refreshReferralInfo(), refreshSettings(), refreshGeo(), updateCalculator()]);
}

// ─── Files ───────────────────────────────────────────────────

const dropZone=$('#drop-zone'), fileInput=$('#file-input');
$('#upload-btn').addEventListener('click', ()=>fileInput.click());
dropZone.addEventListener('dragover', e=>{ e.preventDefault(); dropZone.classList.add('dragover'); });
dropZone.addEventListener('dragleave', ()=>dropZone.classList.remove('dragover'));
dropZone.addEventListener('drop', e=>{ e.preventDefault(); dropZone.classList.remove('dragover'); handleFiles(e.dataTransfer.files); });
fileInput.addEventListener('change', ()=>{ handleFiles(fileInput.files); fileInput.value=''; });

async function handleFiles(fileList) {
    const dt = $('#disk-type-select').value;
    for (const f of fileList) {
        const r = await inv('upload_file', { name:f.name, sizeBytes:f.size, diskType:dt });
        if (r) toast(`"${f.name}" загружен`, 'success');
        else toast(`"${f.name}" — ошибка загрузки`, 'error');
    }
    await Promise.all([refreshFiles(), refreshBalances(), refreshClientStats(), refreshPaymentHistory()]);
}

async function refreshFiles() {
    const files = await inv('get_files');
    const c = $('#file-list');
    if (!files||!files.length) { c.innerHTML='<div class="empty-state"><p>Файлы отсутствуют</p></div>'; return; }
    c.innerHTML = files.map(f=>`
        <div class="file-item" data-id="${f.id}">
            <div class="file-icon">${getFileIcon(f.name)}</div>
            <div class="file-info">
                <div class="file-name">${esc(f.name)}</div>
                <div class="file-meta"><span>${fmtBytes(f.size_bytes)}</span><span>${f.disk_type.toUpperCase()}</span><span>${f.is_credit?'[кредит] ':''}${fmtMoney(f.cost_per_month)}/мес</span></div>
            </div>
            <div class="replica-badges">${'<span class="replica-dot"></span>'.repeat(Math.min(f.replicas,4))}</div>
            <div class="file-cost">${fmtMoney(f.cost_per_month)}/мес</div>
            <div class="file-actions">
                <button class="btn btn-sm btn-outline" onclick="dlFile('${f.id}')">Скачать</button>
                <button class="btn btn-sm btn-danger" onclick="delFile('${f.id}','${esc(f.name)}')">Удалить</button>
            </div>
        </div>`).join('');
}

function getFileIcon(n) { const x=n.split('.').pop().toLowerCase(); return {pdf:'\uD83D\uDCC4',doc:'\uD83D\uDCC3',docx:'\uD83D\uDCC3',xls:'\uD83D\uDCC8',xlsx:'\uD83D\uDCC8',jpg:'\uD83D\uDDBC',jpeg:'\uD83D\uDDBC',png:'\uD83D\uDDBC',mp4:'\uD83C\uDFAC',mp3:'\uD83C\uDFB5',zip:'\uD83D\uDCE6'}[x]||'\uD83D\uDCC1'; }

async function dlFile(id) { const r=await inv('download_file',{fileId:id}); if(r) toast('Загрузка начата','success'); }
async function delFile(id,name) { const r=await inv('delete_file',{fileId:id}); if(r!==null) { toast(`"${name}" удалён`,'success'); await Promise.all([refreshFiles(),refreshBalances(),refreshClientStats()]); } }

// ─── Balance ─────────────────────────────────────────────────

async function refreshBalances() {
    const c = await inv('get_client_balance');
    if (c) {
        $('#client-balance').textContent = c.balance.toFixed(2);
        $('#client-bonus').textContent = c.bonus.toFixed(2);
        state.creditEnabled = c.creditStorageEnabled;
        $('#credit-mult-label').textContent = c.creditStorageEnabled ? 'x1.5' : 'x1.0';

        // Low balance warning
        const wb = $('#low-balance-badge');
        if (c.lowBalanceWarning) wb.classList.remove('hidden'); else wb.classList.add('hidden');

        // Credit period
        const cb = $('#credit-period-badge');
        const cuw = $('#credit-upload-warning');
        if (c.creditAction === 'block_uploads') {
            cb.classList.remove('hidden');
            const h = Math.floor(c.creditTicksRemaining / 12);
            const m = c.creditTicksRemaining % 12;
            $('#credit-timer').textContent = `${h}:${String(m*5).padStart(2,'0')}`;
            cuw.classList.remove('hidden');
        } else if (c.creditAction === 'delete_data') {
            cuw.textContent = 'Кредитный период истёк. Данные удалены.';
            cuw.classList.remove('hidden');
            cb.classList.add('hidden');
        } else {
            cb.classList.add('hidden');
            cuw.classList.add('hidden');
        }
    }
    const k = await inv('get_keeper_balance');
    if (k) {
        $('#keeper-balance').textContent = k.balance.toFixed(2);
        $('#keeper-pending').textContent = k.pending.toFixed(2);
        $('#btn-payout').disabled = !k.canWithdraw;
        if (k.payoutNote) $('#payout-note').textContent = k.payoutNote;
    }
}

$('#btn-topup').addEventListener('click', ()=>$('#modal-topup').classList.remove('hidden'));
$('#btn-cancel-topup').addEventListener('click', ()=>$('#modal-topup').classList.add('hidden'));
$('#modal-topup .modal-overlay').addEventListener('click', ()=>$('#modal-topup').classList.add('hidden'));
$('#topup-amount').addEventListener('input', updateTopup);
$$('.method-btn').forEach(b=>b.addEventListener('click',()=>{ $$('.method-btn').forEach(x=>x.classList.remove('active')); b.classList.add('active'); state.topupMethod=b.dataset.method; updateTopup(); }));

function updateTopup() {
    const a=parseFloat($('#topup-amount').value)||0, comm=state.topupMethod==='card'?a*0.025:0;
    $('#topup-sum').textContent=fmtMoney(a); $('#topup-commission').textContent=fmtMoney(comm); $('#topup-total').textContent=fmtMoney(a+comm);
}

$('#btn-do-topup').addEventListener('click', async()=>{
    const a=parseFloat($('#topup-amount').value)||0;
    if(a<10){toast('Минимум 10 \u20BD','error');return;}
    const r=await inv('topup_client',{amount:a,method:state.topupMethod});
    if(r){toast(`Пополнено: ${fmtMoney(r.amount)}`,'success');$('#modal-topup').classList.add('hidden');await Promise.all([refreshBalances(),refreshPaymentHistory()]);}
});

$('#btn-payout').addEventListener('click', async()=>{
    const r=await inv('request_payout');
    if(r){toast(r,'success');await Promise.all([refreshBalances(),refreshPaymentHistory()]);}
});

// ─── Calculator ─────────────────────────────────────────────

$$('.disk-btn').forEach(b=>b.addEventListener('click',()=>{ $$('.disk-btn').forEach(x=>x.classList.remove('active')); b.classList.add('active'); state.calcDisk=b.dataset.disk; updateCalculator(); }));
$('#calc-volume').addEventListener('input', updateCalculator);
$('#calc-unit').addEventListener('change', updateCalculator);
$('#calc-credit-toggle').addEventListener('change', updateCalculator);

async function updateCalculator() {
    let vol=parseFloat($('#calc-volume').value)||0;
    if($('#calc-unit').value==='mb') vol/=1024;
    const credit=$('#calc-credit-toggle').checked;
    const r=await inv('calculate_storage_cost',{gb:vol,diskType:state.calcDisk,credit});
    if(r){
        $('#calc-monthly').textContent=fmtMoney(r.costPerMonth);
        $('#calc-5min').textContent='~'+fmtMoney(r.costPer5min);
        $('#calc-repl-mult').textContent=`x4 / x${r.creditMultiplier.toFixed(1)}`;
        $('#cmp-hdd').textContent=fmtMoney(r.comparison.hddMonthly);
        $('#cmp-ssd').textContent=fmtMoney(r.comparison.ssdMonthly);
        $('#cmp-nvme').textContent=fmtMoney(r.comparison.nvmeMonthly);
    }
}

// ─── History ─────────────────────────────────────────────────

async function refreshPaymentHistory() {
    const h=await inv('get_payment_history'), c=$('#payment-history');
    if(!h||!h.length){c.innerHTML='<div class="empty-state"><p>Операций пока нет</p></div>';return;}
    const wl={client:'Клиент',keeper:'Хранитель'};
    c.innerHTML=h.slice().reverse().map(x=>`<div class="history-item"><span class="history-wallet">${wl[x.wallet]||x.wallet}</span><span class="history-desc">${esc(x.description)}</span><span class="history-amount ${x.amount>=0?'positive':'negative'}">${x.amount>=0?'+':''}${fmtMoney(x.amount)}</span></div>`).join('');
}

// ─── Keeper / Status ────────────────────────────────────────

$('#keeper-toggle').addEventListener('click', async e=>{
    if(e.target.checked){$('#modal-agreement').classList.remove('hidden');e.target.checked=false;return;}
    const r=await inv('toggle_keeper_mode',{enabled:false});if(r!==null){state.isKeeper=false;updateKeeperUI();}
});

$('#agree-check').addEventListener('change',e=>$('#btn-agree').disabled=!e.target.checked);
$('#btn-agree').addEventListener('click', async()=>{
    const r=await inv('toggle_keeper_mode',{enabled:true});
    if(r!==null){state.isKeeper=true;toast('Режим хранителя активирован','success');$('#modal-agreement').classList.add('hidden');$('#keeper-toggle').checked=true;updateKeeperUI();}
});
$('#btn-decline-agreement').addEventListener('click',()=>$('#modal-agreement').classList.add('hidden'));
$('#modal-agreement .modal-overlay').addEventListener('click',()=>$('#modal-agreement').classList.add('hidden'));

$('#btn-toggle-relay').addEventListener('click', async()=>{
    const cur=$('#btn-toggle-relay').textContent;
    const enable=cur==='Включить';
    const r=await inv('toggle_relay',{enabled:enable});
    if(r!==null) toast(enable?'Relay включён (+2%)':'Relay выключен','success');
    refreshKeeperStats();
});

$('#btn-graceful-shutdown').addEventListener('click', async()=>{
    if(!confirm('Graceful shutdown уведомит сеть. Продолжить?')) return;
    const r=await inv('graceful_shutdown');
    if(r) toast(r,'success');
});

$('#btn-view-agency').addEventListener('click', async()=>{
    const text=await inv('get_legal_agency');
    if(text){$('#legal-title').textContent='Агентский договор';$('#legal-text-content').textContent=text;$('#modal-legal').classList.remove('hidden');}
});
$('#btn-view-offer').addEventListener('click', async()=>{
    const text=await inv('get_legal_offer');
    if(text){$('#legal-title').textContent='Публичная оферта';$('#legal-text-content').textContent=text;$('#modal-legal').classList.remove('hidden');}
});
$('#btn-close-legal').addEventListener('click',()=>$('#modal-legal').classList.add('hidden'));
$('#modal-legal .modal-overlay').addEventListener('click',()=>$('#modal-legal').classList.add('hidden'));

async function refreshKeeperStats() {
    const s=await inv('get_keeper_stats'); if(!s) return;
    state.isKeeper=s.isActive;
    $('#keeper-toggle').checked=s.isActive;
    $('#keeper-rating-pay').textContent=s.ratingPay.toFixed(3);
    $('#keeper-rating-alloc').textContent=s.ratingAlloc.toFixed(3);
    $('#keeper-storage').textContent=fmtGB(s.storageProvidedGb);
    $('#keeper-earnings').textContent=fmtMoney(s.earningsTotal);
    $('#keeper-peers').textContent=s.connectedPeers;
    $('#white-ip-status').textContent=s.hasWhiteIp?'Да':'Нет (серый IP)';
    $('#bootstrap-status').textContent=s.isBootstrap?'Да':'Нет';

    const pctPay=Math.round(s.ratingPay*100);
    $('#rating-fill-pay').style.width=pctPay+'%';

    if(s.relayEnabled){$('#relay-status').textContent='Активен';$('#relay-status').classList.add('green');$('#btn-toggle-relay').textContent='Выключить';$('#relay-bonus-row').classList.remove('hidden');}
    else{$('#relay-status').textContent=s.relayBanned?'Заблокирован':'Выключен';$('#relay-status').classList.remove('green');$('#btn-toggle-relay').textContent='Включить';$('#relay-bonus-row').classList.add('hidden');}

    if(s.bootstrapNote){$('#bootstrap-bonus-row').classList.remove('hidden');$('#bootstrap-bonus-row .green').textContent='+1% к выплате';}else{$('#bootstrap-bonus-row').classList.add('hidden');}

    $('#credit-keeper-status').textContent=s.creditStorageKeeper?'Включено':'Выключено';

    // Warnings panel
    const wp=$('#warnings-panel');
    if(s.warnings&&s.warnings.length){wp.classList.remove('hidden');wp.innerHTML=s.warnings.map(w=>`<div class="warning-item"><span>${esc(w)}</span></div>`).join('');}
    else{wp.classList.add('hidden');}

    updateKeeperUI();
}

function updateKeeperUI() {
    if(state.isKeeper){$('#keeper-panel').classList.remove('inactive');$('#keeper-inactive-msg').classList.add('hidden');}
    else{$('#keeper-panel').classList.add('inactive');$('#keeper-inactive-msg').classList.remove('hidden');}
}

async function refreshClientStats() {
    const s=await inv('get_client_stats'); if(!s) return;
    $('#client-storage').textContent=fmtGB(s.storageUsedGb);
    $('#client-files').textContent=s.filesCount;
    $('#client-monthly-cost').textContent=fmtMoney(s.monthlyCost);
    $('#peers-count').textContent=s.connectedPeers+' узл.';
    $('#credit-client-status').textContent=s.creditStorageEnabled?'Включено':'Выключено';
}

// ─── Referrals ──────────────────────────────────────────────

async function refreshReferralInfo() {
    const i=await inv('get_referral_info'); if(!i) return;
    $('#referral-link').value=i.referralLink;
    $('#referral-code-display').textContent=i.referralCode;
    $('#referral-count').textContent=i.invitedCount;
    $('#referral-earnings').textContent=fmtMoney(i.totalEarnings);
}
$('#btn-copy-referral').addEventListener('click',async()=>{
    try{await navigator.clipboard.writeText($('#referral-link').value);toast('Скопировано','success');}
    catch{const i=$('#referral-link');i.select();document.execCommand('copy');toast('Скопировано','success');}
});

// ─── Settings ───────────────────────────────────────────────

async function refreshSettings() {
    const s=await inv('get_settings'); if(!s) return;
    $('#setting-bootstrap').value=s.bootstrapNodes.join('\n');
    $('#setting-storage-limit').value=s.storageLimitGb;
    $('#setting-autostart').checked=s.autoStart;
    $('#setting-russia').checked=s.russiaOnly;
    $('#setting-credit-client').checked=s.creditStorageClient;
    $('#setting-credit-keeper').checked=s.creditStorageKeeper;

    const w=await inv('get_wallet_info');
    if(w){$('#settings-peer-id').textContent=w.peerId;$('#settings-mnemonic').textContent=w.mnemonic||'(зашифрована)';}

    const g=await inv('check_geo');
    if(g){$('#settings-install-id').textContent=g.installationId;if(g.verified)$('#geo-status').innerHTML='<span class="dot online"></span><span>РФ подтверждена</span>';}
}

$('#btn-show-mnemonic').addEventListener('click',()=>{const el=$('#settings-mnemonic');el.classList.toggle('hidden');$('#btn-show-mnemonic').textContent=el.classList.contains('hidden')?'Показать':'Скрыть';});

$('#setting-credit-client').addEventListener('change', async e=>{
    const r=await inv('toggle_credit_storage_client',{enabled:e.target.checked});
    if(r!==null) toast(r?'Хранение в долг включено (x1.5)':'Хранение в долг выключено','info');
    refreshSettings();
});

$('#setting-credit-keeper').addEventListener('change', async e=>{
    const r=await inv('toggle_credit_storage_keeper',{enabled:e.target.checked});
    if(r!==null) toast(r?'Кредитное хранение включено':'Кредитное хранение выключено','info');
    refreshSettings();
});

$('#credit-client-terms').classList.toggle;

$('#btn-save-settings').addEventListener('click', async()=>{
    const s={
        bootstrapNodes:$('#setting-bootstrap').value.split('\n').filter(x=>x.trim()),
        storageLimitGb:parseInt($('#setting-storage-limit').value)||100,
        autoStart:$('#setting-autostart').checked,
        russiaOnly:$('#setting-russia').checked,
        creditStorageClient:$('#setting-credit-client').checked,
        creditStorageKeeper:$('#setting-credit-keeper').checked,
    };
    const r=await inv('save_settings',{settings:s});
    if(r!==null) toast('Настройки сохранены','success');
});

// ─── Timer ───────────────────────────────────────────────────

function startTimer() {
    state.timerSeconds=300; updateTimerDisplay();
    setInterval(()=>{
        state.timerSeconds--;
        if(state.timerSeconds<=0){ state.timerSeconds=300; onTick(); }
        updateTimerDisplay();
    },1000);
}

function updateTimerDisplay() {
    const m=Math.floor(state.timerSeconds/60), s=state.timerSeconds%60;
    $('#timer-text').textContent=String(m).padStart(2,'0')+':'+String(s).padStart(2,'0');
}

async function onTick() {
    if(!state.initialized) return;
    const r=await inv('simulate_tick');
    if(r) await Promise.all([refreshBalances(), refreshKeeperStats()]);
}

// ─── Periodic Refresh ───────────────────────────────────────

setInterval(()=>{if(state.initialized)refreshClientStats();},15000);

// ─── Agreement text loader ──────────────────────────────────

(async()=>{
    const text=await inv('get_legal_agency');
    if(text) $('#agreement-text-content').textContent=text;
})();

// ─── Init ────────────────────────────────────────────────────

init();
