// VaultKeeper Tauri v2 Frontend
const { invoke } = window.__TAURI__.core;

document.querySelectorAll('.nav-btn').forEach(btn => {
    btn.addEventListener('click', () => {
        document.querySelectorAll('.nav-btn').forEach(b => b.classList.remove('active'));
        document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
        btn.classList.add('active');
        document.getElementById(btn.dataset.tab).classList.add('active');
    });
});

async function refreshStatus() {
    try {
        const status = await invoke('get_status');
        document.getElementById('status').textContent = status.status;
        document.getElementById('balance').textContent = status.balance.toFixed(2) + ' RUB';
        document.getElementById('peers').textContent = status.connected_peers;
    } catch (e) { console.error('Status error:', e); }
}

async function refreshBalance() {
    try {
        const bal = await invoke('get_balance');
        document.getElementById('balance-amount').textContent = bal.balance.toFixed(2);
    } catch (e) { console.error('Balance error:', e); }
}

// File upload
document.getElementById('upload-btn')?.addEventListener('click', () => {
    document.getElementById('file-input').click();
});

// Key generation
document.getElementById('generate-keys')?.addEventListener('click', () => {
    alert('Key generation requires backend call (implement in production)');
});

document.getElementById('backup-keys')?.addEventListener('click', () => {
    const display = document.getElementById('mnemonic-display');
    display.classList.toggle('hidden');
});

refreshStatus();
refreshBalance();
setInterval(refreshStatus, 30000);
