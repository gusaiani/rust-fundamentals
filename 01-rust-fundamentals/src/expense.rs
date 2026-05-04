use serde::{Deserialize, Serialize};
use std::fmt;
use std::collections::BTreeMap;
use std::path::Path;
use chrono::{DateTime, Utc};

#[derive(Serialize, Deserialize, Debug)]
pub struct Expense {
    pub description: String,
    pub amount: f64,
    pub category: String,
    #[serde(default = "Utc::now")]
    pub date: DateTime<Utc>,
}

impl Expense {
    /// Create a new expense.
    pub fn new(description: &str, amount: f64, category: &str) -> Self {
        Self {
            description: description.to_string(),
            amount,
            category: category.to_string(),
            date: Utc::now(),
        }
    }
}

/// Display an expense as: "Coffee — $4.50 [food]"
impl fmt::Display for Expense {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f, 
            "{} {} — ${:.2} [{}]", 
            self.date.format("%Y-%m-%d"),
            self.description, 
            self.amount, 
            self.category)
    }
}

const FILE_PATH: &str = "expenses.json";

pub fn load_expenses() -> Result<Vec<Expense>, Box<dyn std::error::Error>> {
    if !Path::new(FILE_PATH).exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(FILE_PATH)?;
    let expenses: Vec<Expense> = serde_json::from_str(&data)?;
    Ok(expenses)
}

pub fn save_expenses(expenses: &[Expense]) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string_pretty(expenses)?;
    std::fs::write(FILE_PATH, json)?;
    Ok(())
}

pub fn total_by_category(expenses: &[Expense], category: Option<&str>) -> f64 {
    expenses
        .iter()
        // keep an expense if no filter is set, OR its category matches
        .filter(|e| match category {
            None => true,
            Some(c) => e.category == c,
        })
        .map(|e| e.amount)
        .sum()
}

pub fn summary_by_month(expenses: &[Expense]) -> BTreeMap<String, f64> {
    let mut totals: BTreeMap<String, f64> = BTreeMap::new();
    for expense in expenses {
        let month = expense.date.format("%Y-%m").to_string();
        // .entry() either returns the existing value or inserts the default
        *totals.entry(month).or_insert(0.0) += expense.amount;
    }
    totals
}

pub fn export_csv(expenses: &[Expense], path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Writer::from_path opens the file and writes a header row from the struct fields
    let mut writer = csv::Writer::from_path(path)?;
    for expense in expenses {
        writer.serialize(expense)?;
    }
    // .flush() ensures all buffered bytes hit disk before we return 
    writer.flush()?;
    Ok(())
}