use super::invoices::{InvoiceType, list_invoices};
use crate::api::users::AppUser;
use anyhow::{Context, Result, ensure};
use chrono::{Datelike, Duration, Months, NaiveDate};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct InvoiceHistory {
    pub months: Vec<InvoiceMonth>,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Debug)]
pub struct InvoiceMonth {
    pub month: NaiveDate,
    pub sales: Vec<Value>,
    pub purchases: Vec<Value>,
}

#[derive(Debug, PartialEq)]
pub struct MonthlyTotal {
    pub currency: String,
    pub sales: f64,
    pub purchases: f64,
}

impl InvoiceMonth {
    pub fn invoices(&self, kind: InvoiceType) -> &[Value] {
        match kind {
            InvoiceType::Sales => &self.sales,
            InvoiceType::Purchases => &self.purchases,
        }
    }

    pub fn totals(&self) -> Result<Vec<MonthlyTotal>> {
        let mut totals = BTreeMap::<String, MonthlyTotal>::new();
        for kind in [InvoiceType::Sales, InvoiceType::Purchases] {
            for invoice in self.invoices(kind) {
                let body = &invoice["InvoiceBody"];
                let number = body["InvoiceNumber"].as_str().unwrap_or("<unknown>");
                let currency = body["CurrencyCode"].as_str()
                    .filter(|currency| !currency.trim().is_empty())
                    .with_context(|| format!("Invoice {number}: missing or invalid CurrencyCode"))?;
                let amount = body["TotalGrossAmount"].as_f64()
                    .filter(|amount| amount.is_finite())
                    .with_context(|| format!("Invoice {number}: missing, invalid or nonfinite TotalGrossAmount"))?;
                let total = totals.entry(currency.to_string()).or_insert_with(|| MonthlyTotal {
                    currency: currency.to_string(), sales: 0.0, purchases: 0.0,
                });
                let sum = match kind {
                    InvoiceType::Sales => &mut total.sales,
                    InvoiceType::Purchases => &mut total.purchases,
                };
                *sum += amount;
                ensure!(sum.is_finite(), "Nonfinite {kind} total for {currency} in {}", self.month);
            }
        }
        Ok(totals.into_values().collect())
    }
}

pub fn initial_range(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    (today.with_day(1).unwrap().checked_sub_months(Months::new(11)).unwrap(), today)
}

fn validate_range(start: NaiveDate, end: NaiveDate) -> Result<()> {
    ensure!((1..=9999).contains(&start.year()) && (1..=9999).contains(&end.year()),
        "Invoice history dates must have years between 1 and 9999");
    ensure!(start <= end, "Invoice history start date must not be after end date");
    Ok(())
}

pub fn query_windows(start: NaiveDate, end: NaiveDate) -> Result<Vec<(NaiveDate, NaiveDate)>> {
    validate_range(start, end)?;
    let mut windows = Vec::new();
    let mut from = start;
    loop {
        let next = from.with_day(1).unwrap().checked_add_months(Months::new(3))
            .context("Invoice query window exceeds supported dates")?;
        let limit = from.checked_add_signed(Duration::days(90)).context("Invoice query window exceeds supported dates")?;
        let to = end.min(next.pred_opt().context("Invalid invoice query window end")?).min(limit);
        windows.push((from, to));
        if to == end { break; }
        from = to.succ_opt().context("Invalid invoice query window start")?;
    }
    Ok(windows)
}

fn issue_date(invoice: &Value) -> Result<NaiveDate> {
    let number = invoice["InvoiceBody"]["InvoiceNumber"].as_str().unwrap_or("<unknown>");
    let value = invoice["InvoiceBody"]["IssueDate"].as_str()
        .with_context(|| format!("Invoice {number}: missing or invalid IssueDate"))?;
    let prefix = value.get(..10)
        .filter(|_| value.len() == 10 || value.as_bytes().get(10) == Some(&b'T'))
        .with_context(|| format!("Invoice {number}: invalid IssueDate {value:?}"))?;
    let date = NaiveDate::parse_from_str(prefix, "%Y-%m-%d")
        .with_context(|| format!("Invoice {number}: invalid IssueDate {value:?}"))?;
    ensure!((1..=9999).contains(&date.year()) && date.format("%Y-%m-%d").to_string() == prefix,
        "Invoice {number}: invalid IssueDate {value:?}");
    Ok(date)
}

pub fn from_rows(start: NaiveDate, end: NaiveDate, sales: Vec<Value>, purchases: Vec<Value>) -> Result<InvoiceHistory> {
    validate_range(start, end)?;
    let mut months = BTreeMap::<NaiveDate, InvoiceMonth>::new();
    for (kind, rows) in [(InvoiceType::Sales, sales), (InvoiceType::Purchases, purchases)] {
        let mut dated_rows = rows.into_iter().map(|invoice| {
            let date = issue_date(&invoice)?;
            ensure!((start..=end).contains(&date), "Invoice IssueDate {date} is outside history range {start}..{end}");
            Ok((date, invoice))
        }).collect::<Result<Vec<_>>>()?;
        dated_rows.sort_by_cached_key(|(date, invoice)| (
            *date,
            invoice["InvoiceBody"]["InvoiceNumber"].as_str().unwrap_or("").to_string(),
            invoice.to_string(),
        ));
        for (date, invoice) in dated_rows {
            let month = date.with_day(1).unwrap();
            let entry = months.entry(month).or_insert_with(|| InvoiceMonth {
                month, sales: Vec::new(), purchases: Vec::new(),
            });
            match kind {
                InvoiceType::Sales => entry.sales.push(invoice),
                InvoiceType::Purchases => entry.purchases.push(invoice),
            }
        }
    }
    let months: Vec<_> = months.into_values().collect();
    for month in &months { month.totals()?; }
    Ok(InvoiceHistory { months, start, end })
}

pub async fn load(user: &AppUser, start: NaiveDate, end: NaiveDate) -> Result<InvoiceHistory> {
    let mut sales = Vec::new();
    let mut purchases = Vec::new();
    for (from, to) in query_windows(start, end)?.into_iter().rev() {
        let from = from.format("%Y/%m/%d").to_string();
        let to = to.format("%Y/%m/%d").to_string();
        let (window_sales, window_purchases) = tokio::join!(
            list_invoices(user, &InvoiceType::Sales, &from, &to),
            list_invoices(user, &InvoiceType::Purchases, &from, &to),
        );
        sales.extend(window_sales.with_context(|| format!("Could not load sales from {from} to {to}"))?);
        purchases.extend(window_purchases.with_context(|| format!("Could not load purchases from {from} to {to}"))?);
    }
    from_rows(start, end, sales, purchases)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn row(day: &str, number: &str, currency: &str, amount: f64) -> Value {
        json!({"InvoiceBody": {
            "IssueDate": day, "InvoiceNumber": number,
            "CurrencyCode": currency, "TotalGrossAmount": amount,
        }})
    }

    #[test]
    fn initial_range_includes_twelve_calendar_months() {
        assert_eq!(initial_range(date("2026-10-02")), (date("2025-11-01"), date("2026-10-02")));
        assert_eq!(initial_range(date("2024-02-29")), (date("2023-03-01"), date("2024-02-29")));
        assert_eq!(initial_range(date("2025-01-31")), (date("2024-02-01"), date("2025-01-31")));
    }

    #[test]
    fn groups_only_nonempty_months_and_sorts_all_invoices() {
        let a = row("2024-01-04T12:30:00Z", "A", "PLN", 10.0);
        let b = row("2024-01-04", "B", "PLN", 20.0);
        let c = row("2024-01-05", "C", "PLN", 30.0);
        let purchase = row("2024-03-01", "P", "EUR", 4.0);
        let history = from_rows(date("2024-01-01"), date("2024-12-31"),
            vec![c.clone(), b.clone(), a.clone()], vec![purchase.clone()]).unwrap();
        assert_eq!(history.start, date("2024-01-01"));
        assert_eq!(history.end, date("2024-12-31"));
        assert_eq!(history.months.iter().map(|month| month.month).collect::<Vec<_>>(),
            vec![date("2024-01-01"), date("2024-03-01")]);
        assert_eq!(history.months[0].invoices(InvoiceType::Sales), &[a, b, c]);
        assert!(history.months[0].invoices(InvoiceType::Purchases).is_empty());
        assert!(history.months[1].invoices(InvoiceType::Sales).is_empty());
        assert_eq!(history.months[1].invoices(InvoiceType::Purchases), &[purchase]);
    }

    #[test]
    fn sorts_purchases_and_preserves_duplicate_records() {
        let a = row("2024-01-04", "A", "PLN", 10.0);
        let b = row("2024-01-04", "B", "PLN", 20.0);
        let history = from_rows(date("2024-01-01"), date("2024-01-31"), vec![],
            vec![b.clone(), a.clone(), a.clone()]).unwrap();
        assert_eq!(history.months[0].purchases, vec![a.clone(), a, b]);
        assert_eq!(history.months[0].totals().unwrap()[0].purchases, 40.0);
    }

    #[test]
    fn separates_currencies_and_preserves_negative_corrections() {
        let history = from_rows(date("2024-01-01"), date("2024-01-31"), vec![
            row("2024-01-01", "A", "PLN", 100.0),
            row("2024-01-02", "B", "EUR", 20.0),
            row("2024-01-03", "C", "PLN", -150.0),
        ], vec![
            row("2024-01-01", "D", "EUR", -30.0),
            row("2024-01-02", "E", "USD", 5.0),
        ]).unwrap();
        assert_eq!(history.months[0].totals().unwrap(), vec![
            MonthlyTotal { currency: "EUR".into(), sales: 20.0, purchases: -30.0 },
            MonthlyTotal { currency: "PLN".into(), sales: -50.0, purchases: 0.0 },
            MonthlyTotal { currency: "USD".into(), sales: 0.0, purchases: 5.0 },
        ]);
    }

    #[test]
    fn empty_rows_produce_no_months() {
        let history = from_rows(date("2024-01-01"), date("2024-12-31"), vec![], vec![]).unwrap();
        assert!(history.months.is_empty());
    }

    #[test]
    fn rejects_malformed_dates_on_both_sides() {
        for value in [Value::Null, json!(12), json!(""), json!("2024-02-30"),
            json!("2024-1-01"), json!("2024-01-01junk"), json!("0000-01-01"),
            json!("not a date"), json!("2024-01-😀")] {
            let mut invoice = row("2024-01-01", "BAD", "PLN", 1.0);
            invoice["InvoiceBody"]["IssueDate"] = value;
            for kind in [InvoiceType::Sales, InvoiceType::Purchases] {
                let (sales, purchases) = match kind {
                    InvoiceType::Sales => (vec![invoice.clone()], vec![]),
                    InvoiceType::Purchases => (vec![], vec![invoice.clone()]),
                };
                let error = from_rows(date("2024-01-01"), date("2024-12-31"), sales, purchases).unwrap_err();
                assert!(error.to_string().contains("IssueDate"), "{error}");
            }
        }
        assert!(from_rows(date("2024-01-01"), date("2024-12-31"), vec![json!({})], vec![]).is_err());
    }

    #[test]
    fn rejects_missing_invalid_and_nonfinite_amounts_and_currencies() {
        for (field, value) in [
            ("TotalGrossAmount", Value::Null),
            ("TotalGrossAmount", json!("NaN")),
            ("TotalGrossAmount", json!("Infinity")),
            ("TotalGrossAmount", json!("1.00")),
            ("TotalGrossAmount", json!(true)),
            ("CurrencyCode", Value::Null),
            ("CurrencyCode", json!("")),
            ("CurrencyCode", json!("  ")),
            ("CurrencyCode", json!(123)),
        ] {
            let mut invoice = row("2024-01-01", "BAD", "PLN", 1.0);
            invoice["InvoiceBody"][field] = value;
            let error = from_rows(date("2024-01-01"), date("2024-01-31"), vec![], vec![invoice]).unwrap_err();
            assert!(error.to_string().contains(field), "{error}");
        }
        for field in ["CurrencyCode", "TotalGrossAmount"] {
            let mut invoice = row("2024-01-01", "BAD", "PLN", 1.0);
            invoice["InvoiceBody"].as_object_mut().unwrap().remove(field);
            assert!(from_rows(date("2024-01-01"), date("2024-01-31"), vec![invoice], vec![]).is_err());
        }
        let month = InvoiceMonth {
            month: date("2024-01-01"),
            sales: vec![row("2024-01-01", "A", "PLN", f64::MAX), row("2024-01-01", "B", "PLN", f64::MAX)],
            purchases: vec![],
        };
        assert!(month.totals().unwrap_err().to_string().contains("Nonfinite"));
    }

    #[test]
    fn rejects_rows_outside_the_requested_range() {
        assert!(from_rows(date("2024-01-02"), date("2024-01-31"),
            vec![row("2024-01-01", "A", "PLN", 1.0)], vec![]).is_err());
    }

    #[test]
    fn windows_handle_partial_months_year_boundaries_and_leap_days() {
        assert_eq!(query_windows(date("2023-12-15"), date("2024-07-04")).unwrap(), vec![
            (date("2023-12-15"), date("2024-02-29")),
            (date("2024-03-01"), date("2024-05-30")),
            (date("2024-05-31"), date("2024-07-04")),
        ]);
        assert_eq!(query_windows(date("2025-01-31"), date("2025-04-01")).unwrap(), vec![
            (date("2025-01-31"), date("2025-03-31")),
            (date("2025-04-01"), date("2025-04-01")),
        ]);
        assert_eq!(query_windows(date("9999-12-31"), date("9999-12-31")).unwrap(),
            vec![(date("9999-12-31"), date("9999-12-31"))]);
    }

    #[test]
    fn windows_have_at_most_three_calendar_months_and_no_gaps_or_overlap() {
        for (start, end) in [
            (date("0001-01-01"), date("0002-01-01")),
            (date("2023-11-30"), date("2025-04-07")),
            (date("2024-02-29"), date("2024-02-29")),
            (date("9998-10-12"), date("9999-12-31")),
        ] {
            let windows = query_windows(start, end).unwrap();
            assert_eq!(windows.first().unwrap().0, start);
            assert_eq!(windows.last().unwrap().1, end);
            let mut covered = 0;
            for &(from, to) in &windows {
                assert!(from <= to);
                assert!((to - from).num_days() <= 90);
                let months = (to.year() - from.year()) * 12 + to.month() as i32 - from.month() as i32 + 1;
                assert!((1..=3).contains(&months));
                covered += (to - from).num_days() + 1;
            }
            for pair in windows.windows(2) {
                assert_eq!(pair[0].1.succ_opt().unwrap(), pair[1].0);
            }
            assert_eq!(covered, (end - start).num_days() + 1);
        }
    }

    #[test]
    fn reported_november_to_january_request_is_split_at_ninety_days() {
        assert_eq!(query_windows(date("2025-11-01"), date("2026-01-31")).unwrap(), vec![
            (date("2025-11-01"), date("2026-01-30")),
            (date("2026-01-31"), date("2026-01-31")),
        ]);
        for month in 1..=12 {
            let start = NaiveDate::from_ymd_opt(2025, month, 1).unwrap();
            let end = start.checked_add_months(Months::new(12)).unwrap();
            for (from, to) in query_windows(start, end).unwrap() {
                assert!((to - from).num_days() <= 90);
            }
        }
    }

    #[test]
    fn rejects_reversed_and_out_of_bounds_ranges() {
        for (start, end) in [
            (date("2024-02-01"), date("2024-01-01")),
            (NaiveDate::from_ymd_opt(0, 1, 1).unwrap(), date("2024-01-01")),
            (date("2024-01-01"), NaiveDate::from_ymd_opt(10000, 1, 1).unwrap()),
        ] {
            assert!(query_windows(start, end).is_err());
            assert!(from_rows(start, end, vec![], vec![]).is_err());
        }
    }
}
