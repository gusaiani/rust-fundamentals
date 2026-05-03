mod expense;

use expense::{Expense, load_expenses, save_expenses, total_by_category};
use std::io::{self, Write};

/// Commands the user can type in the REPL.
enum Command {
    Add {
        description: String,
        amount: f64,
        category: String,
    },
    List,
    Total {
        category: Option<String>,
    },
    Delete {
        index: usize,
    },
    Help,
    Quit,
}


fn parse_command(input: &str) -> Option<Command> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let mut parts = input.splitn(2, ' ');
    let cmd = parts.next()?;
    let rest = parts.next().unwrap_or("");

    match cmd {
        "quit" | "q" => Some(Command::Quit),
        "help" | "h" => Some(Command::Help),
        "list" | "ls" => Some(Command::List),
        "delete" | "del" | "rm" => {
            // .parse::<usize>() returns Result; .ok() → Option, ? bails in None
            let index = rest.parse::<usize>().ok()?;
            Some(Command::Delete { index })
        }
        // TODO: Implement "add" parsing.
        //   1. Split `rest` into at most 3 parts: amount, category, description
        //   2. Parse amount as f64
        //   3. Return None if parsing fails
        "add" => {
            // splitn(3) → at most 3 chunks; the 3rd keeps any remaining spaces
            let mut parts = rest.splitn(3, ' ');
            let amount_str = parts.next()?;
            let category = parts.next()?;
            let description = parts.next()?;

            // .parse::<f64>() returns Result; .ok() converts to Option, then ? bails on None
            let amount = amount_str.parse::<f64>().ok()?;

            Some(Command::Add {
                description: description.to_string(),
                amount,
                category: category.to_string(),
            })
        }
        "total" => {
            let category = if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            };
            Some(Command::Total { category })
        }
        _ => None,
    }
}

/// Print a help message showing available commands.
fn print_help() {
    println!("Commands:");
    println!("  add <amount> <category> <description>  — Add an expense");
    println!("  list                                    — List all expenses");
    println!("  total [category]                        — Show total (optionally by category)");
    println!("  help                                    — Show this message");
    println!("  quit                                    — Exit");
}

/// Prompt the user and return their input.
fn prompt(msg: &str) -> Result<String, io::Error> {
    print!("{msg}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Expense Tracker 💰");
    println!("Type 'help' for commands.\n");

    // TODO: Load existing expenses from file.
    // Replace this with a call to load_expenses() and handle the Result.
    let mut expenses: Vec<Expense> = match load_expenses() {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("Warning: failed to load expenses: {e}");
            Vec::new()
        }
    };

    loop {
        let input = prompt("> ")?;

        let command = match parse_command(&input) {
            Some(cmd) => cmd,
            None => {
                if !input.is_empty() {
                    println!("Unknown command. Type 'help' for usage.");
                }
                continue;
            }
        };

        match command {
            Command::Help => print_help(),
            Command::Quit => {
                println!("Bye!");
                break;
            }
            Command::Add { description, amount, category } => {
                let expense = Expense::new(&description, amount, &category);
                // {expense} uses the Display impl: "Coffee — $4.50 [food]"
                println!("Added: {expense}");
                expenses.push(expense);
                if let Err(e) = save_expenses(&expenses) {
                    eprintln!("Warning: failed to save: {e}");
                }
            }
            Command::List => {
                if expenses.is_empty() {
                    println!("No expenses yet.");
                } else {
                    // .enumerate() yields (index, &expense) pairs starting at 0
                    for (i, expense) in expenses.iter().enumerate() {
                        println!("{i}: {expense}");
                    }
                }
            }
            Command::Total { category } => {
                // .as_deref() converts Option<String> → Option<&str>
                let total = total_by_category(&expenses, category.as_deref());
                match category {
                    Some(cat) => println!("Total for [{cat}]: ${total:.2}"),
                    None => println!("Total: ${total:.2}"),
                }
            }
            Command::Delete { index } => {
                if index >= expenses.len() {
                    println!("No expense at index {index}.")
                } else {
                    // .remove() shifts later items down and returns the removed value
                    let removed = expenses.remove(index);
                    println!("Deleted: {removed}");
                    if let Err(e) = save_expenses(&expenses) {
                        eprintln!("Warning. failed to save: {e}")
                    }
                }
            }
        }
    }
    Ok(())
}
