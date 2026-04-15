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
    Help,
    Quit,
}

/// Parse a line of user input into a Command.
///
/// Expected formats:
///   add <amount> <category> <description...>
///   list
///   total [category]
///   help
///   quit
///
/// TODO: Split the input, match on the first word, and construct the right variant.
/// Return None if the input is invalid.
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
        // TODO: Implement "add" parsing.
        //   1. Split `rest` into at most 3 parts: amount, category, description
        //   2. Parse amount as f64
        //   3. Return None if parsing fails
        "add" => {
            todo!()
        }
        // TODO: Implement "total" parsing.
        //   If `rest` is empty, category is None.
        //   Otherwise, category is Some(rest.to_string()).
        "total" => {
            todo!()
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
fn prompt(msg: &str) -> String {
    print!("{msg}");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn main() {
    println!("Expense Tracker 💰");
    println!("Type 'help' for commands.\n");

    // TODO: Load existing expenses from file.
    // Replace this with a call to load_expenses() and handle the Result.
    let mut expenses: Vec<Expense> = Vec::new();

    loop {
        let input = prompt("> ");

        let command = match parse_command(&input) {
            Some(cmd) => cmd,
            None => {
                if !input.is_empty() {
                    println!("Unknown command. Type 'help' for usage.");
                }
                continue;
            }
        };

        // TODO: Match each command variant and execute the right action.
        // - Add: create an Expense, push to the vec, save to file, print confirmation
        // - List: print each expense with its index (use .iter().enumerate())
        // - Total: call total_by_category() and print the result
        // - Help: call print_help()
        // - Quit: save and break
        match command {
            Command::Help => print_help(),
            Command::Quit => {
                println!("Bye!");
                break;
            }
            Command::Add { description, amount, category } => {
                todo!()
            }
            Command::List => {
                todo!()
            }
            Command::Total { category } => {
                todo!()
            }
        }
    }
}
