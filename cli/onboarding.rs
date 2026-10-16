use crate::api::users::{Language, create::create_app_user};
use crate::api::customers::create::{ContractorLookup, lookup_customer, preview_customer, prompt_digits, save_customer};
use crate::api::users::update::{edit_optional, edit_optional_digits, edit_optional_hourly_rate, edit_settlement_type};
use crate::login::AuthUser;
use anyhow::Context;
use crossterm::style::Stylize;
use inquire::{Confirm, Password, PasswordDisplayMode, Select, Text, validator::Validation};

pub const BANK_NAMES: [&str; 7] = [
    "Alior Bank",
    "Bank Millennium",
    "BNP Paribas",
    "ING Bank Śląski",
    "mBank",
    "PKO Bank Polski",
    "Santander Bank Polska"
];

pub async fn onboard(user: &AuthUser) -> anyhow::Result<()> {
    println!();
    println!("It looks like we haven't worked together just yet.");
    println!("We just need a few details, we promise it will be just a couple of minutes now and it'll be done from now on.");
    println!();
    println!("{}", "Let's start with your details:".bold());
    let mut nip = prompt_digits("NIP:", "1234567890", 10)?;
    let email = Text::new("Email:").with_initial_value(user.email.as_deref().unwrap_or("")).prompt()?;
    let phone = Text::new("Phone:").with_initial_value(user.phone.as_deref().unwrap_or("")).with_placeholder("+48111222333").prompt()?;
    let language = Select::new("Preferred language:", vec!["English", "Polish"]).prompt()?;
    let language = if language == "Polish" { Language::Pl } else { Language::En };

    println!();
    println!("{}", "Your company".bold());
    let contractor = loop {
        println!("Looking up your company in the government registers...");
        let found = lookup_customer::<ContractorLookup>(&nip, "gov/contractors", &[("nip", &nip), ("profile", "user")]).await?;
        if let Some(found) = found.filter(|found| found.active) {
            let mut contractor = found.contractor;
            preview_customer(&contractor.name, &contractor.nip, contractor.regon.as_deref(), &contractor.address_l1, &contractor.country_code);
            if Confirm::new("Is this your company?").with_default(true).prompt()? {
                contractor.notes = Some("seller".to_string());
                break contractor;
            }
        } else {
            println!("No active company found for this NIP.");
        }
        nip = prompt_digits("Company NIP:", "1234567890", 10)?;
    };
    println!();
    let default_item_name = edit_optional("[Optional] Description of your company main service/product (will save time when creating a new invoice):", None)?;
    let default_hourly_rate = edit_optional_hourly_rate(None)?;
    let settlement_type = edit_settlement_type(None)?;
    println!();
    println!("{}", "Connection to KSeF".bold());
    let border = format!("+{}+", "-".repeat(88));
    println!("{border}");
    for line in [
        "1. Open https://ap.ksef.mf.gov.pl and sign in for your company's NIP.",
        "2. Tokeny > Generuj token: name it KSeF Bot; allow invoice issuing,",
        "   invoice viewing and session history (UPO).",
        "3. Click Generuj token, then Odśwież. Copy the token shown once and paste it here."
    ] {
        println!("| {line:<86} |");
    }
    println!("{border}");
    println!("{}", "Is this safe?".bold());
    println!("A token is KSeF's supported way to let KSeF Bot work with your invoices.");
    println!("We store it to connect on your behalf, with only the permissions you select.");
    println!("You stay in control: revoke it in Tokeny > Lista tokenów > Unieważnij to prevent new logins.");
    let ksef_api_token = Password::new("KSeF token (required):")
        .with_display_mode(PasswordDisplayMode::Masked)
        .without_confirmation()
        .with_validator(|value: &str| Ok(if value.trim().is_empty() {
            Validation::Invalid("A KSeF token is required.".into())
        } else {
            Validation::Valid
        }))
        .prompt()?;
    println!();
    println!("{}", "Banking".bold());
    let mut banks = vec!["Skip"];
    banks.extend(BANK_NAMES);
    let bank = Select::new("[Optional] Bank branch:", banks).with_page_size(8).prompt()?;
    let bank_name = if bank == "Skip" { None } else { Some(bank) };
    let bank_account_number = edit_optional_digits("[Optional] Bank account number (26 digits):", None, 26)?;

    create_app_user(&serde_json::json!({
        "id": nip,
        "email": email.trim(),
        "phone": phone.trim(),
        "language": language,
        "defaultItemName": default_item_name,
        "defaultHourlyRate": default_hourly_rate,
        "settlementType": settlement_type,
        "ksefApiToken": ksef_api_token.trim(),
        "bankName": bank_name,
        "bankAccountNumber": bank_account_number
    })).await.context("Error configuring first time user")?;
    save_customer(&nip, &contractor).await.context("Error configuring user contractor data")?;
    println!("Your user is configured, you should be good to login");
    Ok(())
}