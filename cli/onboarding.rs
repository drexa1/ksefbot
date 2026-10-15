use crate::api::users::{Language, create::create_app_user};
use crate::login::AuthUser;
use inquire::{Select, Text};

pub async fn onboard(user: &AuthUser) -> anyhow::Result<()> {
    let nip = Text::new("NIP:").prompt()?;
    let email = Text::new("Email:").with_initial_value(user.email.as_deref().unwrap_or("")).prompt()?;
    let phone = Text::new("Phone:").with_initial_value(user.phone.as_deref().unwrap_or("")).with_placeholder("+48111222333").prompt()?;
    let language = Select::new("Preferred language:", vec!["English", "Polish"]).prompt()?;
    let language = if language == "Polish" { Language::Pl } else { Language::En };
    create_app_user(nip.trim(), email.trim(), phone.trim(), language).await?;
    println!("Your user is now configured");
    Ok(())
}
