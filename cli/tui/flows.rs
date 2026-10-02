use crate::api::invoices::create::{SalesInvoice, UploadInvoiceResult, create_invoice, download_receipt, load_invoice_parties, preview_sales_invoice, submit_invoice, upload_invoice};
use crate::api::users::AppUser;
use crate::tui::prompt::Prompter;
use anyhow::Result;
use std::fs::{create_dir_all, read_dir, read_to_string, write};
use std::io;
use std::path::{Path, PathBuf};

pub async fn prompt_create_invoice(app_user: &AppUser, prompter: &mut impl Prompter) -> Result<()> {
    // Import previously generated (but not submitted) .xml
    let generated_files = generated_invoice_files()?;
    let import_invoice = !generated_files.is_empty() && prompter.select(
        "Create a new invoice or import a generated XML?",
        &["ðŸ“„ Create a new invoice".to_string(), "ðŸ“‚ Import a generated XML".to_string()]
    )? == "ðŸ“‚ Import a generated XML";
    let (mut new_invoice, imported_path) = if import_invoice {
        let Some((invoice, path)) = import_generated_invoice(&generated_files, prompter)? else {
            prompter.pause()?;
            return Ok(());
        };
        (invoice, Some(path))
    } else {
        // Fetch invoice counterparties
        let invoice_parties = load_invoice_parties(app_user).await?;
        let customer = if invoice_parties.customers.len() == 1 {
            &invoice_parties.customers[0]
        } else {
            let customer_choices: Vec<String> = invoice_parties.customers.iter().enumerate().map(|(index, customer)| {
                format!("{}. {} (NIP: {})", index + 1, customer.name, customer.nip.as_deref().unwrap())
            }).collect();
            let selected = prompter.select("Select customer", &customer_choices)?;
            let customer_index = selected.split_once(". ")
                .map(|(index, _)| index.parse::<usize>().unwrap() - 1)
                .unwrap();
            &invoice_parties.customers[customer_index]
        };
        // Prompt hours worked
        let hours_worked = loop {
            let value = prompter.text("Hours worked:", "160")?;
            match value.trim().parse::<u32>() {
                Ok(hours) if hours > 0 => break hours,
                _ => prompter.info("The number of hours must be greater than zero.")?,
            }
        };
        // Preview on screen
        let preview = preview_sales_invoice(app_user, hours_worked)?;
        prompter.info(&format!(
            "ðŸ”Ž Invoice for [{} hours at {:.2} PLN/h]: {:.2} PLN net + {:.2} PLN VAT = {:.2} PLN gross",
            hours_worked, preview.hourly_rate, preview.total_net, preview.total_vat, preview.total_gross
        ))?;
        // Generate .xml
        let new_invoice = create_invoice(app_user, &invoice_parties.user_contractor, customer, hours_worked).await?;
        // Upload to cloud backend
        if prompter.confirm("Save this invoice in your online vault?", true)? {
            let notes = "Invoice generated from CLI client";
            match upload_invoice(app_user, &new_invoice, notes).await? {
                UploadInvoiceResult::Uploaded(invoice_id) => {
                    prompter.info(&format!("Invoice '{invoice_id}' uploaded for user '{}'", app_user.id))?;
                }
                UploadInvoiceResult::AlreadyExists(message) => {
                    prompter.info(&format!("âœ… {message}"))?;
                }
            }
        }
        // Save .xml to application folder
        if prompter.confirm("Save invoice .xml to application folder?", true)? {
            let home = std::env::var_os("USERPROFILE").unwrap();
            let app_folder = PathBuf::from(home).join(".ksefbot").join("generated");
            create_dir_all(&app_folder)?;
            let file_stem = format!("{}-{}", new_invoice.month_name, new_invoice.year);
            let original_path = app_folder.join(format!("{file_stem}.xml"));
            let path = if original_path.exists() {
                let choice = prompter.select(
                    "An invoice file for this month already exists. What shall we do?",
                    &["1. Overwrite existing file".to_string(), "2. Save a new file".to_string()]
                )?;
                match choice.as_str() {
                    "1. Overwrite existing file" => Some(original_path),
                    "2. Save a new file" => {
                        let mut suffix = 2;
                        let path = loop {
                            let candidate = app_folder.join(format!("{file_stem}-{suffix}.xml"));
                            if !candidate.exists() {
                                break candidate;
                            }
                            suffix += 1;
                        };
                        Some(path)
                    }
                    _ => None,
                }
            } else {
                Some(original_path)
            };
            if let Some(path) = path {
                write(&path, &new_invoice.xml)?;
                prompter.info(&format!("ðŸ“‚ Invoice .xml saved to {}", path.display()))?;
            }
        }
        (new_invoice, None)
    };
    // Submit to KSeF
    if prompter.confirm("Submit this invoice to KSeF?", false)? {
        let submission = submit_invoice(app_user, &new_invoice).await?;
        new_invoice.submission = Some(submission);
        if let Some(path) = imported_path {
            let submitted_path = move_to_submitted(&path)?;
            prompter.info(&format!("ðŸ“‚ Invoice XML moved to {}", submitted_path.display()))?;
        }
        prompter.info("Invoice submitted to KSeF.")?;
        prompter.info(&format!("Invoice number: {}", new_invoice.invoice_number))?;
        let submission = new_invoice.submission.as_ref().unwrap();
        prompter.info(&format!("Invoice reference: {}", submission.invoice_reference_number))?;
        prompter.info(&format!("KSeF session: {}", submission.session_reference_number))?;
        if prompter.confirm("Download the KSeF receipt?", true)? {
            let save_path = download_receipt(app_user, &new_invoice).await?;
            prompter.info(&format!("ðŸ“‚ Submission receipt saved to {}", save_path.display()))?;
        }
    } else {
        prompter.info("Invoice created but not submitted.")?;
    }
    prompter.pause()
}

fn generated_invoice_files() -> Result<Vec<PathBuf>> {
    let home = std::env::var_os("USERPROFILE").unwrap();
    let generated_folder = PathBuf::from(home).join(".ksefbot").join("generated");
    create_dir_all(&generated_folder)?;
    let mut files: Vec<_> = read_dir(&generated_folder)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    files.retain(|path| path.is_file() && path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("xml")));
    files.sort();
    Ok(files)
}

fn import_generated_invoice(files: &[PathBuf], prompter: &mut impl Prompter) -> Result<Option<(SalesInvoice, PathBuf)>> {
    let mut choices: Vec<String> = files.iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    choices.push("Back â†©ï¸".to_string());
    let selected = prompter.select("Select a generated invoice XML", &choices)?;
    if selected == "Back â†©ï¸" {
        return Ok(None);
    }
    let index = files.iter().position(|path| path.file_name().unwrap().to_string_lossy() == selected).unwrap();
    let xml = read_to_string(&files[index])?;
    Ok(Some((SalesInvoice::from_xml(xml)?, files[index].clone())))
}

fn move_to_submitted(source: &Path) -> Result<PathBuf> {
    let home = std::env::var_os("USERPROFILE").unwrap();
    let submitted_folder = PathBuf::from(home).join(".ksefbot").join("submitted");
    create_dir_all(&submitted_folder)?;
    let file_name = source.file_name().unwrap();
    let target = submitted_folder.join(file_name);
    let target = if target.exists() {
        let stem = source.file_stem().unwrap().to_string_lossy();
        let extension = source.extension().unwrap().to_string_lossy();
        let mut suffix = 2;
        loop {
            let candidate = submitted_folder.join(format!("{stem}-{suffix}.{extension}"));
            if !candidate.exists() {
                break candidate;
            }
            suffix += 1;
        }
    } else {
        target
    };
    std::fs::rename(source, &target)?;
    Ok(target)
}
