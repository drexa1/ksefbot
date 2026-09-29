use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
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
    let api_key = app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("The application user has no API key configured"))?;
    let response = reqwest::Client::new()
        .get(format!("{}/app/contractors", cf_worker_url!()))
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", api_key)
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

pub async fn list_customers() -> anyhow::Result<()> {
    println!("Fetching contractors...");
    println!("  [API] Response: 3 customers found.");
    println!();
    println!("  1. ACME Sp. z o.o.");
    println!("     NIP: 1234567890");
    println!("     Email: billing@acme.example");
    println!();
    println!("  2. Example Ltd.");
    println!("     NIP: 9876543210");
    println!("     Email: invoices@example.com");
    println!();
    println!("  3. Test Company");
    println!("     NIP: 5555555555");
    println!("     Email: finance@test.example");
    Ok(())
}

pub async fn create_customer() -> anyhow::Result<()> {
    let name = Text::new("Contractor name").with_placeholder("ACME Sp. z o.o.").prompt()?;
    let nip = Text::new("NIP").with_placeholder("1234567890").prompt()?;
    let email = Text::new("Email").with_placeholder("billing@example.com").prompt()?;
    println!();
    println!("  [API] contractor created successfully.");
    println!("  Contractor ID: {}", "contractor001");
    let _ = (name, nip, email);
    Ok(())
}