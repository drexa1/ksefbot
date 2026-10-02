use super::{assemble_address_l1, other_contractors, parse_address_l1};
use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use crossterm::style::Stylize;
use inquire::{Confirm, Select, Text};
use serde::Serialize;

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

pub async fn edit_customer(app_user: &AppUser) -> anyhow::Result<bool> {
    let mut contractors = other_contractors(app_user).await?;
    if contractors.is_empty() {
        println!("  API Response: No customers found.");
        println!();
        return Ok(true);
    }
    let mut contractor = if contractors.len() == 1 {
        contractors.remove(0)
    } else {
        let choices: Vec<String> = contractors.iter().enumerate().map(|(index, contractor)| format!("{}. {}", index + 1, contractor.name)).collect();
        let mut choices = choices;
        choices.push("Back ↩️".to_string());
        let selected = Select::new("Select customer to edit:", choices.clone()).with_page_size(15).prompt()?;
        if selected == "Back ↩️" {
            return Ok(false);
        }
        let index = choices.iter().position(|choice| choice == &selected).unwrap();
        contractors.remove(index)
    };
    let original = contractor.clone();
    let (mut city, mut postal_code, mut street, mut building_number, mut apartment_number) = parse_address_l1(&contractor.address_l1);
    loop {
        contractor.address_l1 = assemble_address_l1(&city, &postal_code, street.as_deref(), &building_number, apartment_number.as_deref());
        println!();
        let keys = ["Name", "NIP", "PESEL", "REGON", "Internal identifier", "City", "Postal code", "Street", "Building number", "Apartment number", "Notes"];
        let values = [
            contractor.name.clone(),
            contractor.nip.clone().unwrap_or_default(),
            contractor.pesel.clone().unwrap_or_default(),
            contractor.regon.clone().unwrap_or_default(),
            contractor.internal_identifier.clone().unwrap_or_default(),
            city.clone(),
            postal_code.clone(),
            street.clone().unwrap_or_default(),
            building_number.clone(),
            apartment_number.clone().unwrap_or_default(),
            contractor.notes.clone().unwrap_or_default(),
        ];
        let mut fields: Vec<String> = keys.iter().zip(values.iter())
            .map(|(key, value)| format!("{}: {value}", key.bold()))
            .collect();
        if contractor != original {
            fields.push("✅ Save changes".to_string());
        }
        fields.push("❌ Cancel".to_string());
        let field = Select::new("Edit customer", fields.clone()).with_page_size(15).prompt()?;
        let selected_index = fields.iter().position(|choice| choice == &field).unwrap();
        match keys.get(selected_index).copied().unwrap_or_default() {
            "Name" => contractor.name = edit_required("Name:", &contractor.name)?,
            "NIP" => contractor.nip = Some(edit_digits("NIP:", contractor.nip.as_deref(), 10)?),
            "PESEL" => contractor.pesel = Some(edit_digits("PESEL:", contractor.pesel.as_deref(), 11)?),
            "REGON" => contractor.regon = edit_optional_digits("REGON:", contractor.regon.as_deref(), 9)?,
            "Internal identifier" => contractor.internal_identifier = Some(edit_required("Internal identifier:", contractor.internal_identifier.as_deref().unwrap_or(""))?),
            "City" => city = edit_required("City:", &city)?,
            "Postal code" => postal_code = edit_required("Postal code:", &postal_code)?,
            "Street" => street = edit_optional("Street:", street.as_deref())?,
            "Building number" => building_number = edit_required("Building number:", &building_number)?,
            "Apartment number" => apartment_number = edit_optional("Apartment number:", apartment_number.as_deref())?,
            "Notes" => contractor.notes = edit_optional("Notes:", contractor.notes.as_deref())?,
            _ if field.starts_with("✅") => break,
            _ => return Ok(false),
        }
    }
    contractor.address_l1 = assemble_address_l1(&city, &postal_code, street.as_deref(), &building_number, apartment_number.as_deref());
    println!();
    if contractor == original {
        println!("  No changes made.");
        return Ok(true);
    }
    if !Confirm::new("Save changes to this contractor?").with_default(true).prompt()? {
        return Ok(true);
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
    let response = crate::api::client::http_client()
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
    Ok(true)
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
