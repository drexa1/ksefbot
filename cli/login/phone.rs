use crate::login::{AuthUser, load_session, save_session};
use anyhow::Context;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use inquire::Text;
use crate::api::client::http_client;

pub async fn login_with_phone() -> anyhow::Result<Option<AuthUser>> {
    let last_phone = load_session().and_then(|session| session.last_phone).unwrap_or_default();
    let mut phone = None;
    for _ in 0..3 {
        let value = Text::new("Phone number:").with_initial_value(&last_phone).with_placeholder("+48111222333").prompt()?;
        let value = value.trim();
        if value.starts_with("+48") && value.len() == 12 && value[3..].chars().all(|c| c.is_ascii_digit()) {
            phone = Some(value.to_string());
            break;
        }
        println!("Please enter a valid phone number, e.g: +48111222333");
    }
    let phone = phone.context("Too many invalid phone number attempts.")?;
    // Call to request a code
    let response = login_request(reqwest::Method::POST).json(&serde_json::json!({ "phone": phone })).send().await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    if status == reqwest::StatusCode::NOT_FOUND {
        crate::onboarding::onboard(&AuthUser { email: None, name: None, phone: Some(phone) }).await?;
        return Ok(None);
    }
    if !status.is_success() {
        anyhow::bail!("{}", body["error"].as_str().unwrap_or("Could not send verification code"));
    }
    println!("  📲 We've sent a Whatsapp verification code to {phone}.");
    for _ in 0..3 {
        let code = Text::new("Please enter the verification code:").prompt()?;
        // Call to validate the received code
        let response = login_request(reqwest::Method::POST).json(&serde_json::json!({ "phone": phone, "code": code })).send().await?;
        let status = response.status();
        let body: serde_json::Value = response.json().await?;
        if status.is_success() {
            let token = body["token"].as_str().context("Missing phone login token")?;
            let user = resume_phone(token).await?;
            save_session("phone", token, Some(&phone))?;
            return Ok(Some(user));
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
