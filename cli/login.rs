use crate::{google_client_id, google_client_secret, microsoft_client_id};
use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use inquire::{Confirm, Password, Text};
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
    #[strum(to_string = "Sign in with Microsoft account")]
    Microsoft,
    #[strum(to_string = "Sign in with Google account")]
    Google,
    #[strum(to_string = "Sign in with your phone")]
    Phone
}

impl LoginMethod {
    fn session_key(&self) -> Option<&'static str> {
        match self {
            LoginMethod::Microsoft => Some("microsoft"),
            LoginMethod::Google => Some("google"),
            LoginMethod::Phone => None
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
pub struct AuthUser {
    pub email: Option<String>,
    pub name: Option<String>,
    pub phone: Option<String>
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredSession {
    method: String,
    refresh_token: String
}

// -------------------------------------------------------------------------------------------------
// Login with SSO
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
    let http_client = reqwest::Client::new();
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
    let token: MSTokenResponse = reqwest::Client::new().post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token").form(&[
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
    let user: AuthUser = reqwest::Client::new().get("https://openidconnect.googleapis.com/v1/userinfo")
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
/// Returns `None` if there is no stored session for this method (or resuming failed), so the
/// caller can fall back to the regular interactive login flow.
pub async fn try_resume_method(method: &LoginMethod) -> Option<AuthUser> {
    let session = load_session()?;
    if Some(session.method.as_str()) != method.session_key() {
        return None;
    }
    let result = match session.method.as_str() {
        "microsoft" => refresh_microsoft(&session.refresh_token).await,
        "google" => refresh_google(&session.refresh_token).await,
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

// -------------------------------------------------------------------------------------------------
// Login with email
// -------------------------------------------------------------------------------------------------

pub async fn login_with_phone_loop() -> Result<AuthUser> {
    let phone = Text::new("Phone number:").with_placeholder("+48111222333").prompt()?;
    if account_exists(&phone)? {
        println!("Account found.");
        let password = Password::new("Verification code:").prompt()?;
        return login_with_phone(&phone, &password);
    }
    println!("No account found for {phone}.");
    let create = Confirm::new("Would you like to create an account?")
        .with_default(true)
        .with_help_message("Create an account with this phone")
        .prompt()?;
    if create {
        create_account(&phone)?;
        println!("Account created successfully");
        println!("We've sent a verification code to: {phone} - Please verify your phone and continue to log in.");
        anyhow::bail!("Phone verification required");
    } else {
        anyhow::bail!("Account creation cancelled");
    }
}

fn account_exists(phone: &str) -> Result<bool> {
    println!("  [API] GET /users");
    println!("  [API] phone = {phone}");
    println!("  [API] Response: account not found");
    // Change to true to test the password flow.
    Ok(false)
}

fn create_account(phone: &str) -> Result<()> {
    println!("Creating account...");
    println!("  [API] POST /users");
    println!("  [API] phone = {phone}");
    println!("  [API] Creating account...");
    Ok(())
}

fn login_with_phone(phone: &str, password: &str) -> Result<AuthUser> {
    let _ = password;
    println!("  [API] Response: authentication successful");
    let user = AuthUser {
        name: Some("Dummy User".to_owned()),
        email: Some("dummy@example.com".to_owned()),
        phone: Some(phone.to_owned())
    };
    Ok(user)
}