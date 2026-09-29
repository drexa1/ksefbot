use crate::{google_client_id, google_client_secret, microsoft_client_id};
use anyhow::{Context, Result};
use inquire::{Confirm, Password, Text};
use oauth2::{
    AuthUrl,
    AuthorizationCode,
    ClientId,
    ClientSecret,
    CsrfToken,
    RedirectUrl,
    Scope,
    TokenResponse,
    TokenUrl,
    basic::BasicClient};
use strum::{Display, EnumIter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone, Display, EnumIter)]
pub enum LoginMethod {
    #[strum(to_string = "Sign in with Microsoft account")] Microsoft,
    #[strum(to_string = "Sign in with Google account")] Google,
    #[strum(to_string = "Created account with your e-mail")] Email
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuthUser {
    pub name: Option<String>,
    pub email: String
}

// -------------------------------------------------------------------------------------------------
// Login with SSO
// -------------------------------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct MSTokenResponse {
    id_token: String
}

pub async fn login_with_microsoft() -> Result<AuthUser> {
    println!("Opening Microsoft authentication...");
    let listener = TcpListener::bind("127.0.0.1:0").await.context("Failed to bind OAuth callback listener")?;
    let port = listener.local_addr().context("Failed to determine OAuth callback port")?.port();
    let redirect_uri = format!("http://localhost:{port}");
    let code_verifier = {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        use rand::{rngs::OsRng, RngCore};
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    };
    let code_challenge = {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(code_verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(hash)
    };
    let state = {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        use rand::{rngs::OsRng, RngCore};
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    };
    let mut authorize_url = url::Url::parse("https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize").context("Failed to create Microsoft authorization URL")?;
    authorize_url.query_pairs_mut()
        .append_pair("client_id", microsoft_client_id!())
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_mode", "query")
        .append_pair("scope", "openid profile email User.Read")
        .append_pair("state", &state)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256");
    webbrowser::open(authorize_url.as_str()).context("Failed to open browser")?;
    let (mut stream, _) = listener.accept().await.context("Failed to accept OAuth callback")?;
    let mut buffer = [0u8; 8192];
    let bytes_read = stream.read(&mut buffer).await.context("Failed to read OAuth callback")?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let request_target = request.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .context("Invalid OAuth callback request")?;
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
        anyhow::bail!("Microsoft login failed: {}{}", error, error_description.map(|description| format!(" ({description})")).unwrap_or_default());
    }
    let received_state = received_state.context("Microsoft callback did not contain state")?;
    if received_state != state {
        anyhow::bail!("OAuth state mismatch");
    }
    let code = code.context("Microsoft callback did not contain authorization code")?;
    let http_client = reqwest::Client::new();
    let token: MSTokenResponse = http_client.post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
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
    let token_parts: Vec<&str> = token.id_token.split('.').collect();
    if token_parts.len() != 3 {
        anyhow::bail!("Invalid Microsoft ID token");
    }
    let payload = token_parts[1];
    let payload = {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD.decode(payload).context("Failed to decode Microsoft ID token")?
    };
    let claims: AuthUser = serde_json::from_slice(&payload).context("Failed to parse Microsoft ID token claims")?;
    Ok(claims)
}

pub async fn login_with_google() -> Result<AuthUser> {
    println!("Opening Google authentication...");
    let listener = TcpListener::bind("127.0.0.1:0").await.context("Failed to bind OAuth callback listener")?;
    let port = listener.local_addr().context("Failed to determine OAuth callback port")?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}");
    let client = BasicClient::new(ClientId::new(google_client_id!().to_owned()))
        .set_client_secret(ClientSecret::new(google_client_secret!().to_owned()))
        .set_auth_uri(AuthUrl::new("https://accounts.google.com/o/oauth2/auth".to_owned())?)
        .set_token_uri(TokenUrl::new("https://oauth2.googleapis.com/token".to_owned())?).set_redirect_uri(RedirectUrl::new(redirect_uri.clone())?);
    let (authorize_url, csrf_state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("openid".to_owned()))
        .add_scope(Scope::new("email".to_owned()))
        .add_scope(Scope::new("profile".to_owned()))
        .url();
    webbrowser::open(authorize_url.as_str()).context("Failed to open browser")?;
    let (mut stream, _) = listener.accept().await.context("Failed to accept OAuth callback")?;
    let mut buffer = [0u8; 8192];
    let bytes_read = stream
        .read(&mut buffer)
        .await
        .context("Failed to read OAuth callback")?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let request_target = request.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .context("Invalid OAuth callback request")?;
    let callback_url = url::Url::parse(&format!("http://127.0.0.1{request_target}")).context("Failed to parse OAuth callback URL")?;
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for (key, value) in callback_url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            _ => {}
        }
    }
    let response_body = if let Some(error) = &error {
        format!("<html><body><h1>Google login failed</h1><p>Error: {error}</p></body></html>")
    } else {
        "<html><body><h1>Login successful</h1><p>You can close this window and return to KSeF Bot.</p></body></html>".to_owned()
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {response_body}",
        response_body.len()
    );
    stream.write_all(response.as_bytes()).await.context("failed to send OAuth callback response")?;
    let received_state = state.context("Google callback did not contain state")?;
    if received_state != *csrf_state.secret() {
        anyhow::bail!("OAuth state mismatch");
    }
    let code = code.context("Google callback did not contain authorization code")?;
    let http_client = oauth2::reqwest::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .context("Failed to create OAuth HTTP client")?;
    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .request_async(&http_client)
        .await
        .context("Failed to exchange authorization code for Google token")?;
    println!("Google login successful.");
    let access_token = token.access_token().secret();
    println!("Received Google access token ({} characters).", access_token.len());
    let user: AuthUser = reqwest::Client::new()
        .get("https://openidconnect.googleapis.com/v1/userinfo")
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
// Login with email
// -------------------------------------------------------------------------------------------------

pub async fn login_with_email_loop() -> Result<AuthUser> {
    let email = Text::new("Email address").with_placeholder("you@example.com").prompt()?;
    if account_exists(&email)? {
        println!("Account found.");
        let password = Password::new("Password").prompt()?;
        return login_with_email(&email, &password)
    }
    println!("No account found for {email}.");
    let create = Confirm::new("Would you like to create an account?")
        .with_default(true)
        .with_help_message("Create an account with this email")
        .prompt()?;
    if create {
        create_account(&email)?;
        println!("Account created successfully");
        println!("We've sent a verification link to: {email} - Please verify your email and continue to log in.");
        anyhow::bail!("Email verification required");
    } else {
        anyhow::bail!("Account creation cancelled");
    }
}

fn account_exists(email: &str) -> Result<bool> {
    println!("  [API] GET /auth/account-exists");
    println!("  [API] email = {email}");
    println!("  [API] Response: account not found");
    // Change to true to test the password flow.
    Ok(false)
}

fn create_account(email: &str) -> Result<()> {
    println!("Creating account...");
    println!("  [API] POST /auth/register");
    println!("  [API] email = {email}");
    println!("  [API] Creating account record...");
    println!("  [EMAIL] Verification email sent.");
    Ok(())
}

fn login_with_email(email: &str, password: &str) -> Result<AuthUser> {
    let _ = password;
    println!("  [API] POST /auth/login");
    println!("  [API] email = {email}");
    println!("  [API] password = ********");
    println!("  [API] Response: authentication successful");
    let user = AuthUser {
        name: Some("Dummy User".to_owned()),
        email: "dummy@example.com".to_owned()
    };
    Ok(user)
}