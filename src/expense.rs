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

/// Load expenses from the JSON file.
/// If the file doesn't exist, return an empty Vec.
/// TODO: Read the file and deserialize with serde_json.
pub fn load_expenses() -> Result<Vec<Expense>, Box<dyn std::error::Error>> {
    if !Path::new(FILE_PATH).exists() {
        return Ok(Vec::new());
    }
    todo!()
}

/// Save expenses to the JSON file.
/// TODO: Serialize to pretty JSON and write to the file.
pub fn save_expenses(expenses: &[Expense]) -> Result<(), Box<dyn std::error::Error>> {
    todo!()
}

/// Calculate total spending, optionally filtered by category.
/// If category is None, sum all expenses.
/// TODO: Use iterator methods — .iter().filter().map().sum()
pub fn total_by_category(expenses: &[Expense], category: Option<&str>) -> f64 {
    todo!()
}
