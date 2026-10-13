use crate::{google_client_id, google_client_secret, microsoft_client_id};
use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use inquire::{Password, Text};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, RefreshToken, Scope,
    TokenResponse, TokenUrl, basic::BasicClient,
};
use rand::Rng;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use strum::{Display, EnumIter, IntoEnumIterator};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone, Display, EnumIter, PartialEq)]
pub enum LoginMethod {
    #[strum(to_string = "Sign in with Microsoft account")] Microsoft,
    #[strum(to_string = "Sign in with Google account")] Google,
    #[strum(to_string = "Sign in with your phone")] Phone
}
impl LoginMethod {
    fn session_key(&self) -> Option<&'static str> {
        match self {
            LoginMethod::Microsoft => Some("microsoft"),
            LoginMethod::Google => Some("google"),
            LoginMethod::Phone => Some("phone")
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredSession {
    method: String,
    refresh_token: String
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
pub struct AuthUser {
    pub email: Option<String>,
    pub name: Option<String>,
    pub phone: Option<String>
}

// -------------------------------------------------------------------------------------------------
// Login with Microsoft
// -------------------------------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct MSTokenResponse {
    id_token: String,
    refresh_token: Option<String>
}

pub async fn login_with_microsoft() -> Result<AuthUser> {
    let listener = TcpListener::bind("127.0.0.1:0").await.context("Failed to bind OAuth callback listener")?;
    let port = listener.local_addr().context("Failed to determine OAuth callback port")?.port();
    let redirect_uri = format!("http://localhost:{port}");
    let code_verifier = {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    };
    let code_challenge = {
        let hash = Sha256::digest(code_verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(hash)
    };
    let state = {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    };
    let mut authorize_url = url::Url::parse("https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize").context("Failed to create Microsoft auth URL")?;
    authorize_url.query_pairs_mut()
        .append_pair("client_id", microsoft_client_id!())
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_mode", "query")
        .append_pair("scope", "openid profile email User.Read offline_access")
        .append_pair("state", &state)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256");
    webbrowser::open(authorize_url.as_str()).context("Failed to open browser")?;
    let (mut stream, _) = listener.accept().await.context("Failed to accept OAuth callback")?;
    let mut buffer = [0u8; 8192];
    let bytes_read = stream.read(&mut buffer).await.context("Failed to read OAuth callback")?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let request_target = request.lines().next().and_then(|line| line.split_whitespace().nth(1)).context("Invalid OAuth callback request")?;
    let callback_url = url::Url::parse(&format!("http://127.0.0.1{request_target}")).context("Failed to parse OAuth callback URL")?;
    let mut code = None;
    let mut received_state = None;
    let mut error = None;
    let mut error_description = None;
    for (key, value) in callback_url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => received_state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            "error_description" => error_description = Some(value.into_owned()),
            _ => {}
        }
    }
    let response = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 53\r\nConnection: close\r\n\r\nMicrosoft login successful. You can close this window.";
    stream.write_all(response).await.context("Failed to send OAuth callback response")?;
    stream.shutdown().await.context("Failed to close OAuth callback connection")?;
    if let Some(error) = error {
        anyhow::bail!("Microsoft login failed: {}{}",error,error_description.map(|description| format!(" ({description})")).unwrap_or_default());
    }
    let received_state = received_state.context("Microsoft callback did not contain state")?;
    if received_state != state {
        anyhow::bail!("OAuth state mismatch");
    }
    let code = code.context("Microsoft callback did not contain authorization code")?;
    let http_client = crate::api::client::http_client();
    let token: MSTokenResponse = http_client.post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token").form(&[
            ("client_id", microsoft_client_id!()),
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("code_verifier", code_verifier.as_str()),
        ])
        .send()
        .await
        .context("Failed to exchange Microsoft authorization code")?
        .error_for_status()
        .context("Microsoft token exchange failed")?
        .json()
        .await
        .context("Failed to parse Microsoft token response")?;
    if let Some(refresh_token) = &token.refresh_token {
        save_session("microsoft", refresh_token)?;
    }
    decode_ms_id_token(&token.id_token)
}

async fn refresh_microsoft(refresh_token: &str) -> Result<AuthUser> {
    let token: MSTokenResponse = crate::api::client::http_client().post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token").form(&[
            ("client_id", microsoft_client_id!()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", "openid profile email User.Read offline_access"),
        ])
        .send()
        .await
        .context("Failed to refresh Microsoft token")?
        .error_for_status()
        .context("Microsoft token refresh failed")?
        .json()
        .await
        .context("Failed to parse Microsoft token response")?;
    if let Some(refresh_token) = &token.refresh_token {
        save_session("microsoft", refresh_token)?;
    }
    decode_ms_id_token(&token.id_token)
}

fn decode_ms_id_token(id_token: &str) -> Result<AuthUser> {
    let token_parts: Vec<&str> = id_token.split('.').collect();
    if token_parts.len() != 3 {
        anyhow::bail!("Invalid Microsoft ID token");
    }
    let payload = token_parts[1];
    let payload = {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        URL_SAFE_NO_PAD.decode(payload).context("Failed to decode Microsoft ID token")?
    };
    let claims: AuthUser = serde_json::from_slice(&payload).context("Failed to parse Microsoft ID token claims")?;
    Ok(claims)
}

// -------------------------------------------------------------------------------------------------
// Login with Google
// -------------------------------------------------------------------------------------------------

pub async fn login_with_google() -> Result<AuthUser> {
    let listener = TcpListener::bind("127.0.0.1:0").await.context("Failed to bind OAuth callback listener")?;
    let port = listener.local_addr().context("Failed to determine OAuth callback port")?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}");
    let client = BasicClient::new(ClientId::new(google_client_id!().to_owned()))
        .set_client_secret(ClientSecret::new(google_client_secret!().to_owned()))
        .set_auth_uri(AuthUrl::new("https://accounts.google.com/o/oauth2/auth".to_owned())?)
        .set_token_uri(TokenUrl::new("https://oauth2.googleapis.com/token".to_owned())?)
        .set_redirect_uri(RedirectUrl::new(redirect_uri.clone())?);
    let (authorize_url, csrf_state) = client.authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("openid".to_owned()))
        .add_scope(Scope::new("email".to_owned()))
        .add_scope(Scope::new("profile".to_owned()))
        .url();
    webbrowser::open(authorize_url.as_str()).context("Failed to open browser")?;
    let (mut stream, _) = listener.accept().await.context("Failed to accept OAuth callback")?;
    let mut buffer = [0u8; 8192];
    let bytes_read = stream.read(&mut buffer).await.context("Failed to read OAuth callback")?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let request_target = request.lines().next().and_then(|line| line.split_whitespace().nth(1)).context("Invalid OAuth callback request")?;
    let callback_url = url::Url::parse(&format!("http://127.0.0.1{request_target}")).context("Failed to parse OAuth callback URL")?;
    let mut code = None;
    let mut received_state = None;
    let mut error = None;
    let mut error_description = None;
    for (key, value) in callback_url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => received_state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            "error_description" => error_description = Some(value.into_owned()),
            _ => {}
        }
    }
    if let Some(error) = error {
        let message = format!("Google login failed: {}{}", error, error_description.map(|description| format!(" ({description})")).unwrap_or_default());
        let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{message}", message.len());
        stream.write_all(response.as_bytes()).await.context("Failed to send OAuth callback response")?;
        stream.shutdown().await.context("Failed to close OAuth callback connection")?;
        anyhow::bail!("{message}");
    }
    let response_body = "Google login successful. You can close this window and return to KSeF Bot.";
    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}", response_body.len());
    stream.write_all(response.as_bytes()).await.context("Failed to send OAuth callback response")?;
    stream.shutdown().await.context("Failed to close OAuth callback connection")?;
    let received_state = received_state.context("Google callback did not contain state")?;
    if received_state != *csrf_state.secret() {
        anyhow::bail!("OAuth state mismatch");
    }
    let code = code.context("Google callback did not contain authorization code")?;
    let http_client = oauth2::reqwest::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .context("Failed to create OAuth HTTP client")?;
    let token = client.exchange_code(AuthorizationCode::new(code))
        .request_async(&http_client)
        .await
        .context("Failed to exchange authorization code for Google token")?;
    if let Some(refresh_token) = token.refresh_token() {
        save_session("google", refresh_token.secret())?;
    }
    fetch_google_user(token.access_token().secret()).await
}

async fn refresh_google(refresh_token: &str) -> Result<AuthUser> {
    let client = BasicClient::new(ClientId::new(google_client_id!().to_owned()))
        .set_client_secret(ClientSecret::new(google_client_secret!().to_owned()))
        .set_auth_uri(AuthUrl::new("https://accounts.google.com/o/oauth2/auth".to_owned())?)
        .set_token_uri(TokenUrl::new("https://oauth2.googleapis.com/token".to_owned())?);
    let http_client = oauth2::reqwest::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .context("Failed to create OAuth HTTP client")?;
    let token = client.exchange_refresh_token(&RefreshToken::new(refresh_token.to_owned()))
        .request_async(&http_client)
        .await
        .context("Failed to refresh Google token")?;
    if let Some(refresh_token) = token.refresh_token() {
        save_session("google", refresh_token.secret())?;
    }
    fetch_google_user(token.access_token().secret()).await
}

async fn fetch_google_user(access_token: &str) -> Result<AuthUser> {
    let user: AuthUser = crate::api::client::http_client().get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .context("Failed to retrieve Google user information")?
        .error_for_status()
        .context("Google user information request failed")?
        .json()
        .await
        .context("Failed to parse Google user information")?;
    Ok(user)
}

// -------------------------------------------------------------------------------------------------
// Login with phone
// -------------------------------------------------------------------------------------------------

pub async fn login_with_phone_loop() -> Result<AuthUser> {
    let phone = Text::new("Phone number:").with_placeholder("+48111222333").prompt()?;
    let response = login_with_phone(reqwest::Method::POST).json(&serde_json::json!({ "phone": phone })).send().await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    if !status.is_success() {
        anyhow::bail!("{}", body["error"].as_str().unwrap_or("Could not send verification code"));
    }
    println!("We've sent a Whatsapp verification code to {phone}.");
    for _ in 0..3 {
        let code = Password::new("Verification code:").without_confirmation().prompt()?;
        let response = login_with_phone(reqwest::Method::POST).json(&serde_json::json!({ "phone": phone, "code": code })).send().await?;
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

fn login_with_phone(method: reqwest::Method) -> reqwest::RequestBuilder {
    crate::api::client::http_client()
        .request(method, format!("{}/login/phone", crate::cf_worker_url!()))
        .header("CF-Access-Client-Id", crate::cf_client_id!())
        .header("CF-Access-Client-Secret", crate::cf_client_secret!())
        .header("X-API-Key", crate::api_key!())
}

async fn resume_phone(token: &str) -> Result<AuthUser> {
    let payload = token.split('.').nth(1).context("Invalid phone token")?;
    let claims: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload)?)?;
    let expires_at = claims["exp"].as_i64().context("Phone token has no expiry")?;
    if expires_at <= chrono::Utc::now().timestamp() {
        anyhow::bail!("Phone login expired");
    }
    let user = login_with_phone(reqwest::Method::GET).bearer_auth(token).send().await?.error_for_status()?.json().await?;
    Ok(user)
}

// -------------------------------------------------------------------------------------------------
// SSO persistence
// -------------------------------------------------------------------------------------------------

fn session_path() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").unwrap();
    PathBuf::from(home).join(".ksefbot").join("session.json")
}

fn load_session() -> Option<StoredSession> {
    let content = std::fs::read_to_string(session_path()).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_session(method: &str, refresh_token: &str) -> Result<()> {
    let path = session_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, serde_json::to_string(&StoredSession { method: method.to_string(), refresh_token: refresh_token.to_string() })?)?;
    Ok(())
}

fn clear_session() {
    let _ = std::fs::remove_file(session_path());
}

/// Returns the login method whose session was last persisted, if any.
pub fn last_used_method() -> Option<LoginMethod> {
    let session = load_session()?;
    LoginMethod::iter().find(|method| method.session_key() == Some(session.method.as_str()))
}

/// Tries to silently resume the given method's previous session via its stored refresh token.
/// Returns `None` if no stored session for this method (or resuming failed),
/// so the caller can fall back to the regular interactive login flow.
pub async fn try_resume_method(method: &LoginMethod) -> Option<AuthUser> {
    let session = load_session()?;
    if Some(session.method.as_str()) != method.session_key() {
        return None;
    }
    let result = match session.method.as_str() {
        "microsoft" => refresh_microsoft(&session.refresh_token).await,
        "google" => refresh_google(&session.refresh_token).await,
        "phone" => resume_phone(&session.refresh_token).await,
        _ => return None
    };
    match result {
        Ok(user) => Some(user),
        Err(_) => {
            clear_session();
            None
        }
    }
}