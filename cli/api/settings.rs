use anyhow::Result;
use inquire::Text;

pub async fn edit_profile() -> Result<()> {
    println!("Fetching current profile...");
    println!("  [API] GET /profile");
    println!();
    println!("Current profile:");
    println!("  Name: Jan Kowalski");
    println!("  Company: Example Company Sp. z o.o.");
    println!("  NIP: 1234567890");
    println!("  Email: jan@example.com");
    println!();
    let name = Text::new("Name").with_default("Jan Kowalski").prompt()?;
    let company = Text::new("Company")
        .with_default("Example Company Sp. z o.o.")
        .prompt()?;
    println!();
    println!("Updating profile...");
    println!("  [API] PATCH /profile");
    println!("  [API] name = {name}");
    println!("  [API] company = {company}");
    println!("  [API] Profile updated successfully.");
    Ok(())
}