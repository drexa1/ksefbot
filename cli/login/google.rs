use crate::login::{AuthUser, save_session};
use crate::{google_client_id, google_client_secret};
use anyhow::Context;
use oauth2::basic::BasicClient;
use oauth2::{AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub async fn login_with_google() -> anyhow::Result<AuthUser> {
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
        save_session("google", refresh_token.secret(), None)?;
    }
    fetch_google_user(token.access_token().secret()).await
}

pub async fn refresh_google(refresh_token: &str) -> anyhow::Result<AuthUser> {
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
        save_session("google", refresh_token.secret(), None)?;
    }
    fetch_google_user(token.access_token().secret()).await
}

async fn fetch_google_user(access_token: &str) -> anyhow::Result<AuthUser> {
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
