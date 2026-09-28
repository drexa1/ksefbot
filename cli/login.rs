use crate::{cf_client_id, cf_client_secret, google_client_id, google_client_secret};
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
    #[strum(to_string = "Sign in with Google account")] Google,
    #[strum(to_string = "Sign in with Microsoft account")] Microsoft,
    #[strum(to_string = "Created account with your e-mail")] Email
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuthUser {
    #[serde(rename = "sub")]
    pub name: Option<String>,
    pub email: String
}

// -------------------------------------------------------------------------------------------------
// Login with SSO
// -------------------------------------------------------------------------------------------------

pub async fn login_with_google() -> Result<AuthUser> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let callback = format!("http://127.0.0.1:{port}");
    let response = reqwest::Client::new()
        .get("https://ksefbot-api.druizbarbero.workers.dev/sso/google")
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", "55oUrQjUlwlZYCS30WGfpMMZCiQkfKpt")
        .header("X-OAuth-Callback", &callback)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    webbrowser::open(response["authorizationUrl"].as_str().unwrap())?;
    let (mut stream, _) = listener.accept().await?;
    let mut buffer = [0u8; 8192];
    let bytes_read = stream.read(&mut buffer).await?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let request_target = request.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .context("Invalid OAuth callback request")?;
    let callback_url = url::Url::parse(&format!("http://127.0.0.1{request_target}"))?;
    let error = callback_url.query_pairs()
        .find(|(key, _)| key == "error")
        .map(|(_, value)| value.into_owned());
    let response_body = if let Some(error) = &error {
        format!(
            "<html><body>\
             <h1>Google login failed</h1>\
             <p>error: {error}</p>\
             </body></html>"
        )
    } else {
        "<html><body>\
         <h1>Login successful</h1>\
         <p>You can close this window and return to KSeF Bot.</p>\
         </body></html>".to_owned()
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
    stream.write_all(response.as_bytes()).await?;
    if let Some(error) = error {
        anyhow::bail!("Google login failed: {error}");
    }
    let email = callback_url.query_pairs().find(|(key, _)| key == "email").map(|(_, value)| value.into_owned()).context("Missing email")?;
    let name = callback_url.query_pairs().find(|(key, _)| key == "name").map(|(_, value)| value.into_owned());
    Ok(AuthUser { email, name })
}

pub async fn login_with_microsoft() -> Result<AuthUser> {
    println!("Opening Microsoft authentication...");
    let user = AuthUser {
        name: Some("Dummy User".to_owned()),
        email: "dummy@example.com".to_owned()
    };
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