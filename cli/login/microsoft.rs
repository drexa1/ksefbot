use crate::login::{AuthUser, save_session};
use crate::microsoft_client_id;
use anyhow::Context;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::Rng;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(serde::Deserialize)]
struct MSTokenResponse {
    id_token: String,
    refresh_token: Option<String>
}

pub async fn login_with_microsoft() -> anyhow::Result<AuthUser> {
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

pub async fn refresh_microsoft(refresh_token: &str) -> anyhow::Result<AuthUser> {
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

fn decode_ms_id_token(id_token: &str) -> anyhow::Result<AuthUser> {
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