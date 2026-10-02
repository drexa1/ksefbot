use super::Tui;
use super::widgets::{confirm, edit_optional, edit_optional_digits, edit_required, select_index};
use crate::api::users::{AppUser, Language, SettlementType};
use crate::{cf_client_id, cf_client_secret, cf_worker_url};
use anyhow::Result;

fn language_label(language: &Option<Language>) -> &'static str {
    match language {
        Some(Language::En) => "English",
        Some(Language::Pl) => "Polish",
        None => ""
    }
}

fn settlement_type_label(settlement_type: &Option<SettlementType>) -> &'static str {
    match settlement_type {
        Some(SettlementType::Monthly) => "Monthly",
        Some(SettlementType::Quarterly) => "Quarterly",
        None => ""
    }
}

fn edit_language(terminal: &mut Tui, current: Option<&Language>) -> Result<Option<Language>> {
    let choices = vec!["English".to_string(), "Polish".to_string()];
    let mut selected = usize::from(matches!(current, Some(Language::Pl)));
    Ok(match select_index(terminal, "Language", &choices, &mut selected)? {
        Some(0) => Some(Language::En),
        Some(_) => Some(Language::Pl),
        None => current.cloned(),
    })
}

fn edit_settlement_type(terminal: &mut Tui, current: Option<&SettlementType>) -> Result<Option<SettlementType>> {
    let choices = vec!["Monthly".to_string(), "Quarterly".to_string()];
    let mut selected = usize::from(matches!(current, Some(SettlementType::Quarterly)));
    Ok(match select_index(terminal, "Settlement type", &choices, &mut selected)? {
        Some(0) => Some(SettlementType::Monthly),
        Some(_) => Some(SettlementType::Quarterly),
        None => current.cloned(),
    })
}

fn edit_optional_rate(terminal: &mut Tui, current: Option<f64>) -> Result<Option<f64>> {
    let current_text = current.map(|rate| rate.to_string());
    loop {
        match super::widgets::text_input(terminal, "Default hourly rate", current_text.as_deref().unwrap_or(""))? {
            None => return Ok(current),
            Some(value) if value.trim().is_empty() => return Ok(None),
            Some(value) => match value.trim().parse::<f64>() {
                Ok(rate) => return Ok(Some(rate)),
                Err(_) => super::widgets::message(terminal, "Validation", &["Please enter a valid number, or leave blank.".to_string()])?,
            }
        }
    }
}

pub fn fields(user: &AppUser) -> Vec<String> {
    vec![
        format!("Email: {}", user.email),
        format!("Phone: {}", user.phone.as_deref().unwrap_or("-")),
        format!("Language: {}", language_label(&user.language)),
        format!("Default hourly rate: {}", user.default_hourly_rate.map(|rate| format!("{rate:.2}")).unwrap_or_else(|| "-".to_string())),
        format!("Default item name: {}", user.default_item_name.as_deref().unwrap_or("-")),
        format!("Settlement type: {}", settlement_type_label(&user.settlement_type)),
        format!("Bank name: {}", user.bank_name.as_deref().unwrap_or("-")),
        format!("Bank account number: {}", user.bank_account_number.as_deref().unwrap_or("-")),
    ]
}

pub fn edit_field(terminal: &mut Tui, user: &mut AppUser, index: usize) -> Result<()> {
    match index {
        0 => user.email = edit_required(terminal, "Email", &user.email)?,
        1 => user.phone = Some(edit_required(terminal, "Phone", user.phone.as_deref().unwrap_or(""))?),
        2 => user.language = edit_language(terminal, user.language.as_ref())?,
        3 => user.default_hourly_rate = edit_optional_rate(terminal, user.default_hourly_rate)?,
        4 => user.default_item_name = edit_optional(terminal, "Default item name", user.default_item_name.as_deref())?,
        5 => user.settlement_type = edit_settlement_type(terminal, user.settlement_type.as_ref())?,
        6 => user.bank_name = edit_optional(terminal, "Bank name", user.bank_name.as_deref())?,
        7 => user.bank_account_number = edit_optional_digits(terminal, "Bank account number", user.bank_account_number.as_deref(), 26)?,
        _ => anyhow::bail!("Unknown settings field: {index}"),
    }
    Ok(())
}

pub async fn save_profile(terminal: &mut Tui, app_user: &mut AppUser, user: &AppUser) -> Result<Vec<String>> {
    if user == app_user {
        return Ok(vec!["No changes made.".to_string()]);
    }
    if !confirm(terminal, "Save changes to your profile?", true)? {
        return Ok(vec!["Save cancelled. Your changes are still pending.".to_string()]);
    }
    let response = crate::api::client::http_client()
        .put(format!("{}/app/users", cf_worker_url!()))
        .header("CF-Access-Client-Id", cf_client_id!())
        .header("CF-Access-Client-Secret", cf_client_secret!())
        .header("X-API-Key", app_user.api_key.as_deref().ok_or_else(|| anyhow::anyhow!("User has no API key configured"))?)
        .header("X-User-Id", &app_user.id)
        .header("Accept", "application/json")
        .json(user)
        .send()
        .await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await?;
    if !status.is_success() || body["success"].as_bool() != Some(true) {
        anyhow::bail!("Profile update failed: {}", body["error"].as_str().unwrap_or("unknown error"));
    }
    *app_user = user.clone();
    Ok(vec!["Profile updated successfully.".to_string()])
}
