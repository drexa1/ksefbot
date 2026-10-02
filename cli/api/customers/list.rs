use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use crossterm::style::Stylize;
use serde::Deserialize;

#[path = "create.rs"]
pub mod create;
#[path = "edit.rs"]
pub mod edit;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppContractor {
    pub id: String,
    pub name: String,
    pub nip: Option<String>,
    pub pesel: Option<String>,
    pub regon: Option<String>,
    pub internal_identifier: Option<String>,
    pub country_code: String,
    pub address_l1: String,
    pub notes: Option<String>
}

pub async fn load_contractors(app_user: &AppUser) -> anyhow::Result<Vec<AppContractor>> {
    let response = crate::api::client::http_client()
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
    let contractors = other_contractors(app_user).await?;
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

pub(super) async fn other_contractors(app_user: &AppUser) -> anyhow::Result<Vec<AppContractor>> {
    Ok(load_contractors(app_user).await?.into_iter()
        .filter(|contractor| contractor.nip.as_deref() != Some(app_user.id.as_str()))
        .collect())
}

pub(super) fn assemble_address_l1(city: &str, postal_code: &str, street: Option<&str>, building_number: &str, apartment_number: Option<&str>) -> String {
    format!(
        "{city}, {postal_code}, {}{building_number}{}",
        street.map(|street| format!("{street} ")).unwrap_or_default(),
        apartment_number.map(|apartment_number| format!("/{apartment_number}")).unwrap_or_default()
    )
}

pub(super) fn parse_address_l1(address_l1: &str) -> (String, String, Option<String>, String, Option<String>) {
    let mut parts = address_l1.splitn(3, ", ");
    let city = parts.next().unwrap_or_default().to_string();
    let postal_code = parts.next().unwrap_or_default().to_string();
    let street_and_building = parts.next().unwrap_or_default();
    let (street_and_building, apartment_number) = match street_and_building.split_once('/') {
        Some((street_and_building, apartment_number)) => (street_and_building, Some(apartment_number.to_string())),
        None => (street_and_building, None),
    };
    let (street, building_number) = match street_and_building.rsplit_once(' ') {
        Some((street, building_number)) => (Some(street.to_string()), building_number.to_string()),
        None => (None, street_and_building.to_string()),
    };
    (city, postal_code, street, building_number, apartment_number)
}
