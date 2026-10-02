use super::assemble_address_l1;
use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use crossterm::style::Stylize;
use inquire::{Confirm, Select, Text};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NewContractor {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    nip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pesel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    internal_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    regon: Option<String>,
    country_code: String,
    address_l1: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>
}

pub async fn create_customer(app_user: &AppUser) -> anyhow::Result<()> {
    println!();
    println!("  {}", "Customer identifier".bold());
    let name = prompt_required("Contractor name:", "ACME Sp. z o.o.")?;
    let identifier_type = Select::new("Identifier type:", vec!["NIP", "PESEL", "Internal identifier"]).prompt()?;
    let (nip, pesel, internal_identifier) = match identifier_type {
        "NIP" => (Some(prompt_digits("NIP:", "1234567890", 10)?), None, None),
        "PESEL" => (None, Some(prompt_digits("PESEL:", "12345678901", 11)?), None),
        _ => (None, None, Some(Text::new("Internal identifier:").prompt()?)),
    };
    let regon = prompt_optional_digits(&format!("REGON ({}):", "optional".grey()), "123456785", 9)?;
    println!();
    println!("  {}", "Address".bold());
    let city = prompt_required("City:", "Warszawa")?;
    let postal_code = prompt_required("Postal code:", "03-301")?;
    let street = prompt_optional(&format!("Street ({}):", "optional".grey()), "Jagiellońska")?;
    let building_number = prompt_required("Building number:", "74")?;
    let apartment_number = prompt_optional(&format!("Apartment number ({}):", "optional".grey()), "3")?;
    let address_l1 = assemble_address_l1(&city, &postal_code, street.as_deref(), &building_number, apartment_number.as_deref());
    println!();
    println!("  {}", "Contact & notes".bold());
    let notes = prompt_optional(&format!("Notes ({}, up to 256 characters):", "optional".grey()), "")?;
    let contractor = NewContractor { name, nip, pesel, internal_identifier, regon, country_code: "PL".to_string(), address_l1, notes };
    println!();
    if !Confirm::new("Save this contractor in your online vault?").with_default(true).prompt()? {
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
        anyhow::bail!("Contractor creation failed: {}", body["error"].as_str().unwrap_or("unknown error"));
    }
    println!();
    println!("  ✅ Contractor created successfully.");
    println!("  Contractor ID: {}", body["id"].as_str().unwrap_or("-"));
    Ok(())
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
