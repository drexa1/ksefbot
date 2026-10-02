use super::assemble_address_l1;
use crate::api::users::AppUser;
use crate::tui::prompt::Prompter;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
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

pub async fn create_customer(app_user: &AppUser, prompter: &mut impl Prompter) -> anyhow::Result<()> {
    prompter.info("Customer identifier")?;
    let name = prompt_required(prompter, "Contractor name (e.g. ACME Sp. z o.o.):")?;
    let identifier_type = prompter.select("Identifier type:", &["NIP".to_string(), "PESEL".to_string(), "Internal identifier".to_string()])?;
    let (nip, pesel, internal_identifier) = match identifier_type.as_str() {
        "NIP" => (Some(prompt_digits(prompter, "NIP (e.g. 1234567890):", 10)?), None, None),
        "PESEL" => (None, Some(prompt_digits(prompter, "PESEL (e.g. 12345678901):", 11)?), None),
        _ => (None, None, Some(prompter.text("Internal identifier:", "")?)),
    };
    let regon = prompt_optional_digits(prompter, "REGON (optional, e.g. 123456785):", 9)?;
    prompter.info("Address")?;
    let city = prompt_required(prompter, "City (e.g. Warszawa):")?;
    let postal_code = prompt_required(prompter, "Postal code (e.g. 03-301):")?;
    let street = prompt_optional(prompter, "Street (optional, e.g. JagielloÅ„ska):")?;
    let building_number = prompt_required(prompter, "Building number (e.g. 74):")?;
    let apartment_number = prompt_optional(prompter, "Apartment number (optional, e.g. 3):")?;
    let address_l1 = assemble_address_l1(&city, &postal_code, street.as_deref(), &building_number, apartment_number.as_deref());
    prompter.info("Contact & notes")?;
    let notes = prompt_optional(prompter, "Notes (optional, up to 256 characters):")?;
    let contractor = NewContractor { name, nip, pesel, internal_identifier, regon, country_code: "PL".to_string(), address_l1, notes };
    if !prompter.confirm("Save this contractor in your online vault?", true)? {
        prompter.pause()?;
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
    prompter.info("âœ… Contractor created successfully.")?;
    prompter.info(&format!("Contractor ID: {}", body["id"].as_str().unwrap_or("-")))?;
    prompter.pause()
}

fn prompt_required(prompter: &mut impl Prompter, message: &str) -> anyhow::Result<String> {
    loop {
        let value = prompter.text(message, "")?;
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
        prompter.info("This field is required.")?;
    }
}

fn prompt_optional(prompter: &mut impl Prompter, message: &str) -> anyhow::Result<Option<String>> {
    let value = prompter.text(message, "")?;
    Ok(if value.trim().is_empty() { None } else { Some(value.trim().to_string()) })
}

fn prompt_digits(prompter: &mut impl Prompter, message: &str, length: usize) -> anyhow::Result<String> {
    loop {
        let value = prompter.text(message, "")?;
        if value.len() == length && value.chars().all(|character| character.is_ascii_digit()) {
            return Ok(value);
        }
        prompter.info(&format!("Please enter exactly {length} digits."))?;
    }
}

fn prompt_optional_digits(prompter: &mut impl Prompter, message: &str, length: usize) -> anyhow::Result<Option<String>> {
    loop {
        let value = prompter.text(message, "")?;
        if value.trim().is_empty() {
            return Ok(None);
        }
        if value.len() == length && value.chars().all(|character| character.is_ascii_digit()) {
            return Ok(Some(value));
        }
        prompter.info(&format!("Please enter exactly {length} digits, or leave blank."))?;
    }
}
