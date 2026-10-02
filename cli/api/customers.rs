use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use crossterm::style::Stylize;
use inquire::{Confirm, Select, Text};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContractorUpdate {
    id: String,
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

async fn other_contractors(app_user: &AppUser) -> anyhow::Result<Vec<AppContractor>> {
    Ok(load_contractors(app_user).await?.into_iter()
        .filter(|contractor| contractor.nip.as_deref() != Some(app_user.id.as_str()))
        .collect())
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
    let address_l1 = format!(
        "{city}, {postal_code}, {}{building_number}{}",
        street.map(|street| format!("{street} ")).unwrap_or_default(),
        apartment_number.map(|apartment_number| format!("/{apartment_number}")).unwrap_or_default()
    );
    println!();
    println!("  {}", "Contact & notes".bold());
    let notes = prompt_optional(&format!("Notes ({}, up to 256 characters):", "optional".grey()), "")?;
    let contractor = NewContractor { name, nip, pesel, internal_identifier, regon, country_code: "PL".to_string(), address_l1, notes };
    println!();
    if !Confirm::new("Save this contractor in your online vault?").with_default(true).prompt()? {
        return Ok(());
    }
    let response = reqwest::Client::new()
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

pub async fn edit_customer(app_user: &AppUser) -> anyhow::Result<()> {
    let mut contractors = other_contractors(app_user).await?;
    if contractors.is_empty() {
        println!("  API Response: No customers found.");
        println!();
        return Ok(());
    }
    let choices: Vec<String> = contractors.iter().enumerate().map(|(index, contractor)| format!("{}. {}", index + 1, contractor.name)).collect();
    let mut choices = choices;
    choices.push("Back ↩️".to_string());
    let selected = Select::new("Select a customer to edit:", choices.clone()).with_page_size(15).prompt()?;
    if selected == "Back ↩️" {
        return Ok(());
    }
    let index = choices.iter().position(|choice| choice == &selected).unwrap();
    let mut contractor = contractors.remove(index);
    loop {
        println!();
        let keys = ["Name", "NIP", "PESEL", "REGON", "Internal identifier", "Address", "Notes"];
        let values = [
            contractor.name.clone(),
            contractor.nip.clone().unwrap_or_default(),
            contractor.pesel.clone().unwrap_or_default(),
            contractor.regon.clone().unwrap_or_default(),
            contractor.internal_identifier.clone().unwrap_or_default(),
            contractor.address_l1.clone(),
            contractor.notes.clone().unwrap_or_default(),
        ];
        let mut fields: Vec<String> = keys.iter().zip(values.iter())
            .map(|(key, value)| format!("{}: {value}", key.bold()))
            .collect();
        fields.push("✅ Save changes".to_string());
        fields.push("❌ Cancel".to_string());
        let field = Select::new("Select a field to edit:", fields.clone()).with_page_size(15).prompt()?;
        let selected_index = fields.iter().position(|choice| choice == &field).unwrap();
        match keys.get(selected_index).copied().unwrap_or_default() {
            "Name" => contractor.name = edit_required("Name:", &contractor.name)?,
            "NIP" => contractor.nip = Some(edit_digits("NIP:", contractor.nip.as_deref(), 10)?),
            "PESEL" => contractor.pesel = Some(edit_digits("PESEL:", contractor.pesel.as_deref(), 11)?),
            "REGON" => contractor.regon = edit_optional_digits("REGON:", contractor.regon.as_deref(), 9)?,
            "Internal identifier" => contractor.internal_identifier = Some(edit_required("Internal identifier:", contractor.internal_identifier.as_deref().unwrap_or(""))?),
            "Address" => contractor.address_l1 = edit_required("Address:", &contractor.address_l1)?,
            "Notes" => contractor.notes = edit_optional("Notes:", contractor.notes.as_deref())?,
            _ if field.starts_with("✅") => break,
            _ => return Ok(()),
        }
    }
    println!();
    if !Confirm::new("Save changes to this contractor?").with_default(true).prompt()? {
        return Ok(());
    }
    let update = ContractorUpdate {
        id: contractor.id.clone(),
        name: contractor.name,
        nip: contractor.nip,
        pesel: contractor.pesel,
        internal_identifier: contractor.internal_identifier,
        regon: contractor.regon,
        country_code: contractor.country_code,
        address_l1: contractor.address_l1,
        notes: contractor.notes
    };
    let response = reqwest::Client::new()
        .put(format!("{}/app/contractors", cf_worker_url!()))
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/json")
        .json(&update)
        .send()
        .await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    if !status.is_success() || body["success"].as_bool() != Some(true) {
        anyhow::bail!("Contractor update failed: {}", body["error"].as_str().unwrap_or("unknown error"));
    }
    println!();
    println!("  ✅ Contractor updated successfully.");
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

fn edit_required(message: &str, current: &str) -> anyhow::Result<String> {
    loop {
        let value = Text::new(message).with_initial_value(current).prompt()?;
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
        println!("  {}", "This field is required.".dark_yellow());
    }
}

fn edit_optional(message: &str, current: Option<&str>) -> anyhow::Result<Option<String>> {
    let mut prompt = Text::new(message);
    if let Some(current) = current {
        prompt = prompt.with_initial_value(current);
    }
    let value = prompt.prompt()?;
    Ok(if value.trim().is_empty() { None } else { Some(value.trim().to_string()) })
}

fn edit_digits(message: &str, current: Option<&str>, length: usize) -> anyhow::Result<String> {
    loop {
        let mut prompt = Text::new(message);
        if let Some(current) = current {
            prompt = prompt.with_initial_value(current);
        }
        let value = prompt.prompt()?;
        if value.len() == length && value.chars().all(|character| character.is_ascii_digit()) {
            return Ok(value);
        }
        println!("  {}", format!("Please enter exactly {length} digits.").dark_yellow());
    }
}

fn edit_optional_digits(message: &str, current: Option<&str>, length: usize) -> anyhow::Result<Option<String>> {
    loop {
        let mut prompt = Text::new(message);
        if let Some(current) = current {
            prompt = prompt.with_initial_value(current);
        }
        let value = prompt.prompt()?;
        if value.trim().is_empty() {
            return Ok(None);
        }
        if value.len() == length && value.chars().all(|character| character.is_ascii_digit()) {
            return Ok(Some(value));
        }
        println!("  {}", format!("Please enter exactly {length} digits, or leave blank.").dark_yellow());
    }
}