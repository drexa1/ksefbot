use crate::login::AuthUser;
use crate::{api_key, cf_client_id, cf_client_secret, cf_worker_url};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUser {
    // Identification data
    pub id: String,
    pub email: String,
    pub tier: i32,
    pub language: Option<Language>,
    pub phone: Option<String>,
    pub company_logo: Option<Vec<u8>>,
    // Contractor data
    pub contractor_id: Option<String>,
    // Application
    pub api_key: Option<String>,
    // KSeF integration
    pub ksef_api_token: Option<String>,
    // Invoicing defaults
    pub default_item_name: Option<String>,
    pub default_hourly_rate: Option<f64>,
    pub settlement_type: Option<SettlementType>,
    // Banking integration
    pub bank_name: Option<String>,
    pub bank_account_number: Option<String>,
    pub bank_api_token: Option<String>,
    // DBA
    pub created_at: Option<String>,
    pub updated_at: Option<String>
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    En, Pl
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettlementType {
    Monthly, Quarterly
}

pub async fn get_app_user(logged_user: &AuthUser) -> anyhow::Result<Option<AppUser>> {
    let response = reqwest::Client::new()
        .get(format!("{}/app/users", cf_worker_url!()))
        .query(&[("email", logged_user.email.as_deref().unwrap())])
        .query(&[("onboarding", "true")])
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", api_key!())
        .header("Accept", "application/json")
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response.error_for_status()?;
    let users: Vec<AppUser> = response.json().await?;
    Ok(users.into_iter().next())
}

pub async fn init_app_user(logged_user: &AuthUser) -> anyhow::Result<AppUser> {
    let app_user = AppUser {
        id: "dummy-nip".to_string(),
        email: logged_user.email.clone().unwrap(),
        tier: 0,
        language: Some(Language::En),
        phone: Some("000000000".to_string()),
        company_logo: None,
        contractor_id: Some("dummy-contractor-id".to_string()),
        api_key: None,
        ksef_api_token: None,
        default_item_name: Some("Dummy item".to_string()),
        default_hourly_rate: Some(100.0),
        settlement_type: Some(SettlementType::Monthly),
        bank_name: None,
        bank_account_number: None,
        bank_api_token: None,
        created_at: None,
        updated_at: None
    };
    Ok(app_user)
}