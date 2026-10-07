use super::{AppContractor, assemble_address_line};
use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use crossterm::style::Stylize;
use inquire::{Confirm, Text};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct NewContractor {
    name: String,
    nip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    regon: Option<String>,
    country_code: String,
    #[serde(alias = "addressLine")]
    address_l1: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>
}

#[derive(Deserialize)]
struct ContractorLookup {
    active: bool,
    #[serde(flatten)]
    contractor: NewContractor
}

pub async fn create_customer(app_user: &AppUser) -> anyhow::Result<()> {
    println!();
    let nip = prompt_digits("Customer NIP:", "1234567890", 10)?;
    if let Some(customer) = lookup_customer::<Vec<AppContractor>>(app_user, "app/contractors", &[("nip", &nip), ("ownerId", &app_user.id)]).await?
        .and_then(|customers| customers.into_iter().next()) {
        println!("  ✅ Customer already saved in your online vault.");
        preview_customer(&customer.name, &nip, customer.regon.as_deref(), &customer.address_l1, &customer.country_code);
        println!("  Customer ID: {}", customer.id);
        return Ok(());
    }
    let contractor = match lookup_customer::<ContractorLookup>(app_user, "gov/contractors", &[("nip", &nip), ("profile", "customer")]).await? {
        Some(mut found) if found.active => {
            println!("  ✅ Customer found.");
            preview_customer(&found.contractor.name, &found.contractor.nip, found.contractor.regon.as_deref(), &found.contractor.address_l1, &found.contractor.country_code);
            found.contractor.notes = Some("buyer".to_string());
            found.contractor
        }
        found => {
            if found.is_some() {
                println!("  We found the contractor but it is no longer active.");
            }
            if !Confirm::new(&format!(
                "A company with NIP {nip} could not be found in the government databases.\n\
                 Do you want to create it by filling in the details?:"
            )).with_default(true).prompt()? {
                return Ok(());
            }
            let contractor = prompt_customer_details(nip)?;
            preview_customer(&contractor.name, &contractor.nip, contractor.regon.as_deref(), &contractor.address_l1, &contractor.country_code);
            contractor
        }
    };
    println!();
    if !Confirm::new("Save this customer in your online vault?").with_default(true).prompt()? {
        return Ok(());
    }
    let response = crate::api::client::http_client()
        .post(format!("{}/app/contractors", cf_worker_url!()))
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/json")
        .json(&contractor)
        .send()
        .await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    if !status.is_success() || body["success"].as_bool() != Some(true) {
        anyhow::bail!("Contractor creation failed: {}", body["error"].as_str().unwrap());
    }
    println!();
    println!("  💼 Contractor created successfully.");
    Ok(())
}

async fn lookup_customer<T: DeserializeOwned>(app_user: &AppUser, endpoint: &str, query: &[(&str, &str)]) -> anyhow::Result<Option<T>> {
    let response = crate::api::client::http_client()
        .get(format!("{}/{endpoint}", cf_worker_url!()))
        .query(query)
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/json")
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    Ok(Some(response.error_for_status()?.json().await?))
}

fn preview_customer(name: &str, nip: &str, regon: Option<&str>, address: &str, country: &str) {
    println!();
    println!("  {}", name.bold());
    println!("  NIP: {nip}");
    if let Some(regon) = regon {
        println!("  REGON: {regon}");
    }
    println!("  Address: {address}, {country}");
}

fn prompt_customer_details(nip: String) -> anyhow::Result<NewContractor> {
    println!();
    println!("  {}", "Customer details".bold());
    let name = prompt_required("Contractor name:", "ACME Sp. z o.o.")?;
    let regon = prompt_optional_digits(&format!("REGON ({}):", "optional".grey()), "123456785", 9)?;
    println!();
    println!("  {}", "Address".bold());
    let city = prompt_required("City:", "Warszawa")?;
    let postal_code = prompt_required("Postal code:", "03-301")?;
    let street = prompt_optional(&format!("Street ({}):", "optional".grey()), "Jagiellońska")?;
    let building_number = prompt_required("Building number:", "74")?;
    let apartment_number = prompt_optional(&format!("Apartment number ({}):", "optional".grey()), "3")?;
    let address_l1 = assemble_address_line(&city, &postal_code, street.as_deref(), &building_number, apartment_number.as_deref());
    println!();
    let notes = prompt_optional(&format!("Notes ({}, up to 256 characters):", "optional".grey()), "buyer")?.or_else(|| Some("buyer".to_string()));
    Ok(NewContractor { name, nip, regon, country_code: "PL".to_string(), address_l1, notes })
}

fn prompt_required(message: &str, placeholder: &str) -> anyhow::Result<String> {
    loop {
        let value = Text::new(message).with_placeholder(placeholder).prompt()?;
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
        println!("  {}", "This field is required.".dark_yellow());
    }
}

fn prompt_optional(message: &str, placeholder: &str) -> anyhow::Result<Option<String>> {
    let value = Text::new(message).with_placeholder(placeholder).prompt()?;
    Ok(if value.trim().is_empty() { None } else { Some(value.trim().to_string()) })
}

fn prompt_digits(message: &str, placeholder: &str, length: usize) -> anyhow::Result<String> {
    loop {
        let value = Text::new(message).with_placeholder(placeholder).prompt()?;
        if value.len() == length && value.chars().all(|character| character.is_ascii_digit()) {
            return Ok(value);
        }
        println!("  {}", format!("Please enter exactly {length} digits.").dark_yellow());
    }
}

fn prompt_optional_digits(message: &str, placeholder: &str, length: usize) -> anyhow::Result<Option<String>> {
    loop {
        let value = Text::new(message).with_placeholder(placeholder).prompt()?;
        if value.trim().is_empty() {
            return Ok(None);
        }
        if value.len() == length && value.chars().all(|character| character.is_ascii_digit()) {
            return Ok(Some(value));
        }
        println!("  {}", format!("Please enter exactly {length} digits, or leave blank.").dark_yellow());
    }
}
