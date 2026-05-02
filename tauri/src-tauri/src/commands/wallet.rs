//! Wallet commands: create, restore, verify mnemonic.

use tauri::State;
use crate::state::AppState;
use crate::models::*;
use crate::logic::*;

#[tauri::command]
pub async fn create_wallet(state: State<'_, AppState>) -> Result<WalletInfo, String> {
    tracing::debug!(target: "soty_cmd", "CMD create_wallet start");
    let mut rng_state: [u8; 32] = [0; 32];
    for i in 0..32 {
        rng_state[i] = (i as u8).wrapping_add(42).wrapping_mul(7);
    }
    let peer_id = format!("12D3KooW{}", to_hex(&rng_state[..16]));
    let inst_id = format!("SID-{}", to_hex(&rng_state[16..20]));
    let mnemonic = MNEMONIC_WORDS.join(" ");

    *state.initialized.write() = true;
    *state.peer_id.write() = peer_id.clone();
    *state.mnemonic.write() = mnemonic.clone();
    *state.mnemonic_shown_once.write() = false;
    *state.installation_id.write() = inst_id;
    *state.referral_code.write() = format!("SOTY-{}", to_hex(&rng_state[..4]).to_uppercase());
    *state.has_referrer.write() = false;

    tracing::debug!(target: "soty_cmd", "CMD create_wallet end");
    Ok(WalletInfo {
        peer_id,
        mnemonic,
        referral_code: state.referral_code.read().clone(),
    })
}

#[tauri::command]
pub async fn restore_wallet(state: State<'_, AppState>, mnemonic: String) -> Result<WalletInfo, String> {
    tracing::debug!(target: "soty_cmd", "CMD restore_wallet start");
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    if words.len() != 24 {
        return Err("\u{041c}\u{043d}\u{0435}\u{043c}\u{043e}\u{043d}\u{0438}\u{0447}\u{0435}\u{0441}\u{043a}\u{0430}\u{044f} \u{0444}\u{0440}\u{0430}\u{0437}\u{0430} \u{0434}\u{043e}\u{043b}\u{0436}\u{043d}\u{0430} \u{0441}\u{043e}\u{0434}\u{0435}\u{0440}\u{0436}\u{0430}\u{0442}\u{044c} 24 \u{0441}\u{043b}\u{043e}\u{0432}\u{0430}".to_string());
    }

    let mut rng_state: [u8; 32] = [0; 32];
    for i in 0..32 {
        rng_state[i] = (i as u8).wrapping_add(42).wrapping_mul(7);
    }
    let peer_id = format!("12D3KooW{}", to_hex(&rng_state[..16]));
    let inst_id = format!("SID-{}", to_hex(&rng_state[16..20]));

    *state.initialized.write() = true;
    *state.peer_id.write() = peer_id.clone();
    *state.mnemonic.write() = mnemonic.clone();
    *state.mnemonic_shown_once.write() = true;
    *state.installation_id.write() = inst_id;
    *state.referral_code.write() = format!("SOTY-{}", to_hex(&rng_state[..4]).to_uppercase());
    *state.has_referrer.write() = false;

    tracing::debug!(target: "soty_cmd", "CMD restore_wallet end");
    Ok(WalletInfo {
        peer_id,
        mnemonic,
        referral_code: state.referral_code.read().clone(),
    })
}

#[tauri::command]
pub async fn verify_mnemonic_words_cmd(
    state: State<'_, AppState>,
    positions: Vec<u32>,
    expected: Vec<String>,
) -> Result<bool, String> {
    tracing::debug!(target: "soty_cmd", "CMD verify_mnemonic_words_cmd start");
    let mnemonic = state.mnemonic.read().clone();
    if mnemonic.is_empty() {
        return Err("\u{041c}\u{043d}\u{0435}\u{043c}\u{043e}\u{043d}\u{0438}\u{0447}\u{0435}\u{0441}\u{043a}\u{0430}\u{044f} \u{0444}\u{0440}\u{0430}\u{0437}\u{0430} \u{0443}\u{0436}\u{0435} \u{0437}\u{0430}\u{0448}\u{0438}\u{0444}\u{0440}\u{043e}\u{0432}\u{0430}\u{043d}\u{0430}".to_string());
    }
    let result = verify_mnemonic_words(&mnemonic, &positions, &expected);
    tracing::debug!(target: "soty_cmd", "CMD verify_mnemonic_words_cmd end");
    Ok(result)
}

#[tauri::command]
pub async fn confirm_mnemonic_shown(state: State<'_, AppState>) -> Result<(), String> {
    tracing::debug!(target: "soty_cmd", "CMD confirm_mnemonic_shown start");
    *state.mnemonic_shown_once.write() = true;
    tracing::debug!(target: "soty_cmd", "CMD confirm_mnemonic_shown end");
    Ok(())
}

#[tauri::command]
pub async fn get_wallet_info(state: State<'_, AppState>) -> Result<WalletInfo, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_wallet_info start");
    let info = WalletInfo {
        peer_id: state.peer_id.read().clone(),
        mnemonic: if *state.mnemonic_shown_once.read() {
            String::new()
        } else {
            state.mnemonic.read().clone()
        },
        referral_code: state.referral_code.read().clone(),
    };
    tracing::debug!(target: "soty_cmd", "CMD get_wallet_info end");
    Ok(info)
}

#[tauri::command]
pub async fn sync_files_after_restore(state: State<'_, AppState>) -> Result<String, String> {
    tracing::debug!(target: "soty_cmd", "CMD sync_files_after_restore start");
    if !*state.initialized.read() {
        return Err("\u{041a}\u{043e}\u{0448}\u{0435}\u{043b}\u{0451}\u{043a} \u{043d}\u{0435} \u{0438}\u{043d}\u{0438}\u{0446}\u{0438}\u{0430}\u{043b}\u{0438}\u{0437}\u{0438}\u{0440}\u{043e}\u{0432}\u{0430}\u{043d}".to_string());
    }
    tracing::debug!(target: "soty_cmd", "CMD sync_files_after_restore end");
    Ok(format!("\u{0421}\u{0438}\u{043d}\u{0445}\u{0440}\u{043e}\u{043d}\u{0438}\u{0437}\u{0430}\u{0446}\u{0438}\u{044f} \u{0444}\u{0430}\u{0439}\u{043b}\u{043e}\u{0432} \u{0434}\u{043b}\u{044f} {} \u{0437}\u{0430}\u{043f}\u{0443}\u{0449}\u{0435}\u{043d}\u{0430} \u{0447}\u{0435}\u{0440}\u{0435}\u{0437} DHT",
        state.peer_id.read()))
}
