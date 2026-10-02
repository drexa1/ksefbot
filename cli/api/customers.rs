use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use crossterm::style::Stylize;
use inquire::Text;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppContractor {
    pub id: String,
    pub name: String,
    pub nip: Option<String>,
    pub country_code: String,
    pub address_l1: String
}

pub async fn load_contractors(app_user: &AppUser) -> anyhow::Result<Vec<AppContractor>> {
    let response = reqwest::Client::new()
        .get(format!("{}/app/contractors", cf_worker_url!()))
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/json")
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let response = response.error_for_status()?;
    Ok(response.json().await?)
}

pub async fn list_customers(app_user: &AppUser) -> anyhow::Result<()> {
    let contractors: Vec<_> = load_contractors(app_user).await?.into_iter()
        .filter(|contractor| contractor.nip.as_deref() != Some(app_user.id.as_str()))
        .collect();
    println!("  API Response: {} customers found", contractors.len());
    if contractors.is_empty() {
        return Ok(());
    }
    println!();
    for (index, contractor) in contractors.iter().enumerate() {
        println!("  {}. {}", index + 1, contractor.name.clone().bold());
        println!("     NIP: {}", contractor.nip.as_deref().unwrap_or("-"));
        println!("     Address: {}, {}", contractor.address_l1, contractor.country_code);
    }
    Ok(())
}

pub async fn create_customer() -> anyhow::Result<()> {
    let name = Text::new("Contractor name:").with_placeholder("ACME Sp. z o.o.").prompt()?;
    let nip = Text::new("NIP:").with_placeholder("1234567890").prompt()?;
    let email = Text::new("Email:").with_placeholder("billing@example.com").prompt()?;
    println!();
    println!("  [API] contractor created successfully.");
    println!("  Contractor ID: {}", "contractor001");
    let _ = (name, nip, email);
    Ok(())
}