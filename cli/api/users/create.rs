use super::{AppUser, Language, SettlementType};
use crate::login::AuthUser;

pub async fn init_app_user(logged_user: &AuthUser) -> anyhow::Result<AppUser> {
    let app_user = AppUser {
        id: "dummy-nip".to_string(),
        email: logged_user.email.clone().unwrap(),
        tier: 0,
        language: Some(Language::En),
        phone: Some("000000000".to_string()),
        company_logo: None,
        contractor_id: Some("dummy-contractor-id".to_string()),
        api_key: None,
        ksef_api_token: None,
        default_item_name: Some("Dummy item".to_string()),
        default_hourly_rate: Some(100.0),
        settlement_type: Some(SettlementType::Monthly),
        bank_name: None,
        bank_account_number: None,
        bank_api_token: None,
        created_at: None,
        updated_at: None,
    };
    Ok(app_user)
}
