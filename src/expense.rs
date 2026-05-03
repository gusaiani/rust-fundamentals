use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug)]
pub struct Expense {
    pub description: String,
    pub amount: f64,
    pub category: String,
}

impl Expense {
    /// Create a new expense.
    pub fn new(description: &str, amount: f64, category: &str) -> Self {
        Self {
            description: description.to_string(),
            amount,
            category: category.to_string(),
        }
    }
}

/// Display an expense as: "Coffee — $4.50 [food]"
impl fmt::Display for Expense {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} — ${:.2} [{}]", self.description, self.amount, self.category)
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
