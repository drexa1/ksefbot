use crate::login::google::refresh_google;
use crate::login::microsoft::refresh_microsoft;
use crate::login::phone::resume_phone;
use anyhow::Result;
use std::path::PathBuf;
use strum::{Display, EnumIter, IntoEnumIterator};

pub mod google;
pub mod microsoft;
pub mod phone;

#[derive(Clone, Display, EnumIter, PartialEq)]
pub enum LoginMethod {
    #[strum(to_string = "Sign in with Microsoft account")] Microsoft,
    #[strum(to_string = "Sign in with Google account")] Google,
    #[strum(to_string = "Sign in with your phone")] Phone,
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
// SSO persistence
// -------------------------------------------------------------------------------------------------

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

fn session_path() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").unwrap();
    PathBuf::from(home).join(".ksefbot").join("session.json")
}