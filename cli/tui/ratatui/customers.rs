use super::Tui;
use super::widgets::{confirm, digits_field, edit_digits, edit_optional, edit_optional_digits, edit_required, optional_digits_field, optional_field, required_field, select, select_index};
use crate::api::customers::AppContractor;
use crate::api::users::AppUser;
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use anyhow::Result;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NewContractor {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")] nip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pesel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] internal_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] regon: Option<String>,
    country_code: String,
    address_l1: String,
    #[serde(skip_serializing_if = "Option::is_none")] notes: Option<String>
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContractorUpdate {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")] nip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pesel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] internal_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] regon: Option<String>,
    country_code: String,
    address_l1: String,
    #[serde(skip_serializing_if = "Option::is_none")] notes: Option<String>
}

pub async fn load_customers(app_user: &AppUser) -> Result<Vec<AppContractor>> {
    Ok(crate::api::customers::load_contractors(app_user).await?.into_iter()
        .filter(|contractor| contractor.nip.as_deref() != Some(app_user.id.as_str()))
        .collect())
}

fn assemble_address_l1(city: &str, postal_code: &str, street: Option<&str>, building_number: &str, apartment_number: Option<&str>) -> String {
    format!(
        "{city}, {postal_code}, {}{building_number}{}",
        street.map(|street| format!("{street} ")).unwrap_or_default(),
        apartment_number.map(|apartment_number| format!("/{apartment_number}")).unwrap_or_default()
    )
}

fn parse_address_l1(address_l1: &str) -> (String, String, Option<String>, String, Option<String>) {
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

pub async fn create_customer(terminal: &mut Tui, app_user: &AppUser) -> Result<Vec<String>> {
    let cancelled = vec!["Customer creation cancelled.".to_string()];
    let Some(name) = required_field(terminal, "Contractor name")? else { return Ok(cancelled); };
    let Some(identifier_type) = select(terminal, "Identifier type", &["NIP".to_string(), "PESEL".to_string(), "Internal identifier".to_string()])? else { return Ok(cancelled); };
    let (nip, pesel, internal_identifier) = match identifier_type.as_str() {
        "NIP" => match digits_field(terminal, "NIP", 10)? { Some(nip) => (Some(nip), None, None), None => return Ok(cancelled) },
        "PESEL" => match digits_field(terminal, "PESEL", 11)? { Some(pesel) => (None, Some(pesel), None), None => return Ok(cancelled) },
        _ => match required_field(terminal, "Internal identifier")? { Some(value) => (None, None, Some(value)), None => return Ok(cancelled) },
    };
    let regon = optional_digits_field(terminal, "REGON (optional)", 9)?;
    let Some(city) = required_field(terminal, "City")? else { return Ok(cancelled); };
    let Some(postal_code) = required_field(terminal, "Postal code")? else { return Ok(cancelled); };
    let street = optional_field(terminal, "Street (optional)")?;
    let Some(building_number) = required_field(terminal, "Building number")? else { return Ok(cancelled); };
    let apartment_number = optional_field(terminal, "Apartment number (optional)")?;
    let address_l1 = assemble_address_l1(&city, &postal_code, street.as_deref(), &building_number, apartment_number.as_deref());
    let notes = optional_field(terminal, "Notes (optional, up to 256 characters)")?;
    if !confirm(terminal, "Save this contractor in your online vault?", true)? {
        return Ok(vec!["Customer not saved.".to_string()]);
    }
    let contractor = NewContractor { name, nip, pesel, internal_identifier, regon, country_code: "PL".to_string(), address_l1, notes };
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
    Ok(vec![
        "Contractor created successfully.".to_string(),
        format!("Contractor ID: {}", body["id"].as_str().unwrap_or("-")),
    ])
}

pub async fn edit_customer(terminal: &mut Tui, app_user: &AppUser, customer: &AppContractor) -> Result<Vec<String>> {
    let mut contractor = customer.clone();
    let original = contractor.clone();
    let (mut city, mut postal_code, mut street, mut building_number, mut apartment_number) = parse_address_l1(&contractor.address_l1);
    let mut selected = 0;
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
        let mut fields: Vec<String> = keys.iter().zip(values.iter()).map(|(key, value)| format!("{key}: {value}")).collect();
        if contractor != original {
            fields.push("Save changes".to_string());
        }
        fields.push("Cancel".to_string());
        let Some(selected_index) = select_index(terminal, "Edit customer", &fields, &mut selected)? else { return Ok(vec!["Edit cancelled.".to_string()]); };
        match keys.get(selected_index).copied().unwrap_or_default() {
            "Name" => contractor.name = edit_required(terminal, "Name", &contractor.name)?,
            "NIP" => contractor.nip = Some(edit_digits(terminal, "NIP", contractor.nip.as_deref(), 10)?),
            "PESEL" => contractor.pesel = Some(edit_digits(terminal, "PESEL", contractor.pesel.as_deref(), 11)?),
            "REGON" => contractor.regon = edit_optional_digits(terminal, "REGON", contractor.regon.as_deref(), 9)?,
            "Internal identifier" => contractor.internal_identifier = Some(edit_required(terminal, "Internal identifier", contractor.internal_identifier.as_deref().unwrap_or(""))?),
            "City" => city = edit_required(terminal, "City", &city)?,
            "Postal code" => postal_code = edit_required(terminal, "Postal code", &postal_code)?,
            "Street" => street = edit_optional(terminal, "Street", street.as_deref())?,
            "Building number" => building_number = edit_required(terminal, "Building number", &building_number)?,
            "Apartment number" => apartment_number = edit_optional(terminal, "Apartment number", apartment_number.as_deref())?,
            "Notes" => contractor.notes = edit_optional(terminal, "Notes", contractor.notes.as_deref())?,
            _ if selected_index == keys.len() && contractor != original => break,
            _ => return Ok(vec!["Edit cancelled.".to_string()]),
        }
    }
    contractor.address_l1 = assemble_address_l1(&city, &postal_code, street.as_deref(), &building_number, apartment_number.as_deref());
    if contractor == original {
        return Ok(vec!["No changes made.".to_string()]);
    }
    if !confirm(terminal, "Save changes to this contractor?", true)? {
        return Ok(vec!["Changes discarded.".to_string()]);
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
    Ok(vec!["Contractor updated successfully.".to_string()])
}

pub fn preview(contractor: &AppContractor) -> Vec<String> {
    vec![
        contractor.name.clone(),
        String::new(),
        format!("NIP: {}", contractor.nip.as_deref().unwrap_or("-")),
        format!("PESEL: {}", contractor.pesel.as_deref().unwrap_or("-")),
        format!("REGON: {}", contractor.regon.as_deref().unwrap_or("-")),
        format!("Internal ID: {}", contractor.internal_identifier.as_deref().unwrap_or("-")),
        String::new(),
        "Address".to_string(),
        format!("{}, {}", contractor.address_l1, contractor.country_code),
        String::new(),
        "Notes".to_string(),
        contractor.notes.clone().unwrap_or_else(|| "-".to_string()),
    ]
}
