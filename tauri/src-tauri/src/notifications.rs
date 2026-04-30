//! Email notification module for Соты — SMTP via lettre v0.11.
//!
//! Sends notifications on: penalty, data deletion, graceful shutdown, low balance.
//! Uses the user-configured SMTP settings from AppSettings.

use lettre::{
    Message, SmtpTransport, Transport,
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
};

/// Send an email notification. Returns Ok(()) on success, Err with description on failure.
/// Silently succeeds if notifications are disabled or SMTP is not configured.
pub fn send_notification(
    smtp_host: &str,
    smtp_port: u16,
    smtp_login: &str,
    smtp_password: &str,
    to_email: &str,
    subject: &str,
    body_html: &str,
) -> Result<(), String> {
    if smtp_host.is_empty() || smtp_login.is_empty() || to_email.is_empty() {
        // SMTP not configured — silently skip (no error to user)
        return Ok(());
    }

    let email = Message::builder()
        .from(smtp_login.parse().map_err(|e: lettre::address::AddressError| e.to_string())?)
        .to(to_email.parse().map_err(|e: lettre::address::AddressError| e.to_string())?)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .body(body_html.to_string())
        .map_err(|e| e.to_string())?;

    let transport = SmtpTransport::relay(smtp_host)
        .map_err(|e| e.to_string())?
        .port(smtp_port)
        .credentials(Credentials::new(smtp_login.to_string(), smtp_password.to_string()))
        .build();

    transport.send(&email).map_err(|e| e.to_string())?;
    Ok(())
}

/// Build HTML body for a penalty notification.
pub fn build_penalty_html(peer_id: &str, level: &str, reason: &str, action: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html><body style="font-family:Arial,sans-serif;background:#0a0a0f;color:#fff;padding:20px;">
<div style="max-width:500px;margin:0 auto;background:#16161e;border:1px solid #2a2a3e;border-radius:12px;padding:24px;">
<h2 style="color:#ff4455;">⚠ Штраф</h2>
<p>Узел <code style="color:#39FF14;">{peer_id}</code> получил штраф.</p>
<p><strong>Уровень:</strong> {level}</p>
<p><strong>Причина:</strong> {reason}</p>
<p><strong>Мера:</strong> {action}</p>
<p style="color:#a0a0b8;font-size:12px;margin-top:16px;">Соты — Децентрализованное хранилище</p>
</div></body></html>"#,
        peer_id = peer_id,
        level = level,
        reason = reason,
        action = action,
    )
}

/// Build HTML body for a data-deletion notification.
pub fn build_data_delete_html(peer_id: &str, reason: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html><body style="font-family:Arial,sans-serif;background:#0a0a0f;color:#fff;padding:20px;">
<div style="max-width:500px;margin:0 auto;background:#16161e;border:1px solid #2a2a3e;border-radius:12px;padding:24px;">
<h2 style="color:#ff4455;">Удаление данных</h2>
<p>Данные узла <code style="color:#39FF14;">{peer_id}</code> были удалены.</p>
<p><strong>Причина:</strong> {reason}</p>
<p style="color:#a0a0b8;font-size:12px;margin-top:16px;">Соты — Децентрализованное хранилище</p>
</div></body></html>"#,
        peer_id = peer_id,
        reason = reason,
    )
}

/// Build HTML body for a graceful shutdown notification.
pub fn build_shutdown_html(peer_id: &str, wait_seconds: u64) -> String {
    format!(
        r#"<!DOCTYPE html>
<html><body style="font-family:Arial,sans-serif;background:#0a0a0f;color:#fff;padding:20px;">
<div style="max-width:500px;margin:0 auto;background:#16161e;border:1px solid #2a2a3e;border-radius:12px;padding:24px;">
<h2 style="color:#FF6A00;">Плановое отключение</h2>
<p>Узел <code style="color:#39FF14;">{peer_id}</code> завершает работу.</p>
<p>Graceful shutdown — завершение через <strong>{wait_seconds}</strong> сек.</p>
<p style="color:#a0a0b8;font-size:12px;margin-top:16px;">Соты — Децентрализованное хранилище</p>
</div></body></html>"#,
        peer_id = peer_id,
        wait_seconds = wait_seconds,
    )
}

/// Build HTML body for a low-balance notification.
pub fn build_low_balance_html(peer_id: &str, balance: f64) -> String {
    format!(
        r#"<!DOCTYPE html>
<html><body style="font-family:Arial,sans-serif;background:#0a0a0f;color:#fff;padding:20px;">
<div style="max-width:500px;margin:0 auto;background:#16161e;border:1px solid #2a2a3e;border-radius:12px;padding:24px;">
<h2 style="color:#FF6A00;">Низкий баланс</h2>
<p>Баланс клиента <code style="color:#39FF14;">{peer_id}</code> ниже 10 ₽.</p>
<p><strong>Текущий баланс:</strong> {balance:.2} ₽</p>
<p>Пополните баланс, чтобы избежать удаления данных.</p>
<p style="color:#a0a0b8;font-size:12px;margin-top:16px;">Соты — Децентрализованное хранилище</p>
</div></body></html>"#,
        peer_id = peer_id,
        balance = balance,
    )
}

/// Send a penalty notification. Returns true if email was sent, false if skipped.
pub fn notify_penalty(
    settings: &crate::AppSettings,
    peer_id: &str,
    level: &str,
    reason: &str,
    action: &str,
) -> bool {
    if !settings.notify_enabled || !settings.notify_penalty {
        return false;
    }
    let subject = format!("[Соты] Штраф ({}) — {}", level, reason);
    let html = build_penalty_html(peer_id, level, reason, action);
    send_notification(
        &settings.smtp_host,
        settings.smtp_port,
        &settings.smtp_login,
        &settings.smtp_password_encrypted,
        &settings.notify_email,
        &subject,
        &html,
    ).is_ok()
}

/// Send a data-deletion notification.
pub fn notify_data_delete(settings: &crate::AppSettings, peer_id: &str, reason: &str) -> bool {
    if !settings.notify_enabled || !settings.notify_data_delete {
        return false;
    }
    let subject = format!("[Соты] Удаление данных — {}", reason);
    let html = build_data_delete_html(peer_id, reason);
    send_notification(
        &settings.smtp_host,
        settings.smtp_port,
        &settings.smtp_login,
        &settings.smtp_password_encrypted,
        &settings.notify_email,
        &subject,
        &html,
    ).is_ok()
}

/// Send a shutdown notification.
pub fn notify_shutdown(settings: &crate::AppSettings, peer_id: &str, wait_seconds: u64) -> bool {
    if !settings.notify_enabled || !settings.notify_shutdown {
        return false;
    }
    let subject = "[Соты] Плановое отключение узла".to_string();
    let html = build_shutdown_html(peer_id, wait_seconds);
    send_notification(
        &settings.smtp_host,
        settings.smtp_port,
        &settings.smtp_login,
        &settings.smtp_password_encrypted,
        &settings.notify_email,
        &subject,
        &html,
    ).is_ok()
}

/// Send a low-balance notification.
pub fn notify_low_balance(settings: &crate::AppSettings, peer_id: &str, balance: f64) -> bool {
    if !settings.notify_enabled || !settings.notify_low_balance {
        return false;
    }
    let subject = "[Соты] Низкий баланс".to_string();
    let html = build_low_balance_html(peer_id, balance);
    send_notification(
        &settings.smtp_host,
        settings.smtp_port,
        &settings.smtp_login,
        &settings.smtp_password_encrypted,
        &settings.notify_email,
        &subject,
        &html,
    ).is_ok()
}
