use super::{assemble_address_l1, other_contractors, parse_address_l1};
use crate::api::users::AppUser;
use crate::tui::prompt::Prompter;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
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

pub async fn edit_customer(app_user: &AppUser, prompter: &mut impl Prompter) -> anyhow::Result<()> {
    let mut contractors = other_contractors(app_user).await?;
    if contractors.is_empty() {
        prompter.info("API Response: No customers found.")?;
        prompter.pause()?;
        return Ok(());
    }
    let mut contractor = if contractors.len() == 1 {
        contractors.remove(0)
    } else {
        let mut choices: Vec<String> = contractors.iter().enumerate().map(|(index, contractor)| format!("{}. {}", index + 1, contractor.name)).collect();
        choices.push("Back â†©ï¸".to_string());
        let selected = prompter.select("Select customer to edit:", &choices)?;
        if selected == "Back â†©ï¸" {
            return Ok(());
        }
        let index = choices.iter().position(|choice| choice == &selected).unwrap();
        contractors.remove(index)
    };
    let original = contractor.clone();
    let (mut city, mut postal_code, mut street, mut building_number, mut apartment_number) = parse_address_l1(&contractor.address_l1);
    loop {
        contractor.address_l1 = assemble_address_l1(&city, &postal_code, street.as_deref(), &building_number, apartment_number.as_deref());
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
            .map(|(key, value)| format!("{key}: {value}"))
            .collect();
        if contractor != original {
            fields.push("âœ… Save changes".to_string());
        }
        fields.push("âŒ Cancel".to_string());
        let field = prompter.select("Edit customer", &fields)?;
        let selected_index = fields.iter().position(|choice| choice == &field).unwrap();
        match keys.get(selected_index).copied().unwrap_or_default() {
            "Name" => contractor.name = edit_required(prompter, "Name:", &contractor.name)?,
            "NIP" => contractor.nip = Some(edit_digits(prompter, "NIP:", contractor.nip.as_deref(), 10)?),
            "PESEL" => contractor.pesel = Some(edit_digits(prompter, "PESEL:", contractor.pesel.as_deref(), 11)?),
            "REGON" => contractor.regon = edit_optional_digits(prompter, "REGON:", contractor.regon.as_deref(), 9)?,
            "Internal identifier" => contractor.internal_identifier = Some(edit_required(prompter, "Internal identifier:", contractor.internal_identifier.as_deref().unwrap_or(""))?),
            "City" => city = edit_required(prompter, "City:", &city)?,
            "Postal code" => postal_code = edit_required(prompter, "Postal code:", &postal_code)?,
            "Street" => street = edit_optional(prompter, "Street:", street.as_deref())?,
            "Building number" => building_number = edit_required(prompter, "Building number:", &building_number)?,
            "Apartment number" => apartment_number = edit_optional(prompter, "Apartment number:", apartment_number.as_deref())?,
            "Notes" => contractor.notes = edit_optional(prompter, "Notes:", contractor.notes.as_deref())?,
            _ if field.starts_with("âœ…") => break,
            _ => return Ok(()),
        }
    }
    contractor.address_l1 = assemble_address_l1(&city, &postal_code, street.as_deref(), &building_number, apartment_number.as_deref());
    if contractor == original {
        prompter.info("No changes made.")?;
        prompter.pause()?;
        return Ok(());
    }
    if !prompter.confirm("Save changes to this contractor?", true)? {
        prompter.pause()?;
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
    prompter.info("âœ… Contractor updated successfully.")?;
    prompter.pause()
}

fn edit_required(prompter: &mut impl Prompter, message: &str, current: &str) -> anyhow::Result<String> {
    loop {
        let value = prompter.text(message, current)?;
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
        prompter.info("This field is required.")?;
    }
}

fn edit_optional(prompter: &mut impl Prompter, message: &str, current: Option<&str>) -> anyhow::Result<Option<String>> {
    let value = prompter.text(message, current.unwrap_or(""))?;
    Ok(if value.trim().is_empty() { None } else { Some(value.trim().to_string()) })
}

fn edit_digits(prompter: &mut impl Prompter, message: &str, current: Option<&str>, length: usize) -> anyhow::Result<String> {
    loop {
        let value = prompter.text(message, current.unwrap_or(""))?;
        if value.len() == length && value.chars().all(|character| character.is_ascii_digit()) {
            return Ok(value);
        }
        prompter.info(&format!("Please enter exactly {length} digits."))?;
    }
}

fn edit_optional_digits(prompter: &mut impl Prompter, message: &str, current: Option<&str>, length: usize) -> anyhow::Result<Option<String>> {
    loop {
        let value = prompter.text(message, current.unwrap_or(""))?;
        if value.trim().is_empty() {
            return Ok(None);
        }
        if value.len() == length && value.chars().all(|character| character.is_ascii_digit()) {
            return Ok(Some(value));
        }
        prompter.info(&format!("Please enter exactly {length} digits, or leave blank."))?;
    }
}
