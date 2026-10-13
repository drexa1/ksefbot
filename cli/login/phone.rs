use crate::login::{AuthUser, save_session};
use anyhow::Context;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use inquire::{Password, Text};
use crate::api::client::http_client;

pub async fn login_with_phone() -> anyhow::Result<AuthUser> {
    let phone = loop {
        let value = Text::new("Phone number:").with_placeholder("+48111222333").prompt()?;
        let phone = value.trim();
        if phone.starts_with("+48") && phone.len() == 12 && phone[3..].chars().all(|c| c.is_ascii_digit()) {
            break phone.to_string();
        }
        println!("Enter a valid phone number (e.g. +48111222333)");
    };
    // Call to request a code
    let response = login_request(reqwest::Method::POST).json(&serde_json::json!({ "phone": phone })).send().await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    if !status.is_success() {
        anyhow::bail!("{}", body["error"].as_str().unwrap_or("Could not send verification code"));
    }
    println!("We've sent a Whatsapp verification code to {phone}.");
    for _ in 0..3 {
        let code = Password::new("📲 Please enter the verification code:").without_confirmation().prompt()?;
        // Call to validate the received code
        let response = login_request(reqwest::Method::POST).json(&serde_json::json!({ "phone": phone, "code": code })).send().await?;
        let status = response.status();
        let body: serde_json::Value = response.json().await?;
        if status.is_success() {
            let token = body["token"].as_str().context("Missing phone login token")?;
            let user = resume_phone(token).await?;
            save_session("phone", token)?;
            return Ok(user);
        }
        let error = body["error"].as_str().unwrap_or("Phone login failed");
        if status != reqwest::StatusCode::UNAUTHORIZED && status != reqwest::StatusCode::BAD_REQUEST {
            anyhow::bail!("{error}");
        }
        println!("{error}");
    }
    anyhow::bail!("Verification failed. Please request a new code.");
}

pub async fn resume_phone(token: &str) -> anyhow::Result<AuthUser> {
    let payload = token.split('.').nth(1).context("Invalid phone token")?;
    let claims: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload)?)?;
    let expires_at = claims["exp"].as_i64().context("Phone token has no expiry")?;
    if expires_at <= chrono::Utc::now().timestamp() {
        anyhow::bail!("Phone login expired");
    }
    let user = login_request(reqwest::Method::GET).bearer_auth(token).send().await?.error_for_status()?.json().await?;
    Ok(user)
}

/// Calls the backend API to register a new phone login attempt.
/// The backend will either create a new challenge or validate an already existing one.
fn login_request(method: reqwest::Method) -> reqwest::RequestBuilder {
    http_client().request(method, format!("{}/login/phone", crate::cf_worker_url!()))
        .header("CF-Access-Client-Id", crate::cf_client_id!())
        .header("CF-Access-Client-Secret", crate::cf_client_secret!())
        .header("X-API-Key", crate::api_key!())
}