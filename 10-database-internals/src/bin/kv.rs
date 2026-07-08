//! `kv` — a tiny command-line front end for the store. **Given** (you don't
//! implement this).
//!
//! It opens (or creates) a store in a directory you name and runs one command
//! against it, so you can watch a real LSM tree grow on disk from the shell:
//!
//! ```bash
//! cargo run --bin kv -- ./data put user:1 alice
//! cargo run --bin kv -- ./data put user:2 bob
//! cargo run --bin kv -- ./data get user:1        # -> alice
//! cargo run --bin kv -- ./data del user:1
//! cargo run --bin kv -- ./data get user:1        # -> (not found)
//! cargo run --bin kv -- ./data flush             # force memtable -> SSTable
//! ls ./data                                      # wal.log + NNNNNNNNNN.sst files
//! cargo run --bin kv -- ./data scan              # every live key=value
//! cargo run --bin kv -- ./data compact           # merge SSTables into one
//! cargo run --bin kv -- ./data stats
//! ```
//!
//! Because every `put`/`del` is WAL-synced, you can kill the process at any
//! point and the next command still sees every acknowledged write — that's the
//! crash safety, observable by hand. Values are treated as UTF-8 for display;
//! the store itself stores arbitrary bytes.

use std::process::ExitCode;

use lsmkv::Db;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            eprintln!();
            usage();
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> std::result::Result<(), String> {
    let dir = args.first().ok_or("missing <dir>")?;
    let cmd = args.get(1).ok_or("missing <command>")?.as_str();

    let mut db = Db::open(dir).map_err(|e| format!("open {dir}: {e}"))?;

    match cmd {
        "put" => {
            let key = args.get(2).ok_or("put needs <key> <value>")?;
            let value = args.get(3).ok_or("put needs <key> <value>")?;
            db.put(key.as_bytes(), value.as_bytes()).map_err(io)?;
            println!("ok");
        }
        "get" => {
            let key = args.get(2).ok_or("get needs <key>")?;
            match db.get(key.as_bytes()).map_err(io)? {
                Some(v) => println!("{}", String::from_utf8_lossy(&v)),
                None => {
                    println!("(not found)");
                    return Ok(());
                }
            }
        }
        "del" | "delete" => {
            let key = args.get(2).ok_or("del needs <key>")?;
            db.delete(key.as_bytes()).map_err(io)?;
            println!("ok");
        }
        "scan" => {
            for (k, v) in db.scan().map_err(io)? {
                println!("{}={}", String::from_utf8_lossy(&k), String::from_utf8_lossy(&v));
            }
        }
        "flush" => {
            db.flush().map_err(io)?;
            println!("ok");
        }
        "compact" => {
            db.compact().map_err(io)?;
            println!("ok");
        }
        "stats" => {
            let s = db.stats();
            println!("memtable_entries  {}", s.memtable_entries);
            println!("memtable_bytes    {}", s.memtable_bytes);
            println!("sstables          {}", s.sstables);
            println!("sstable_records   {}", s.sstable_records);
            println!("next_seq          {}", s.next_seq);
        }
        other => return Err(format!("unknown command '{other}'")),
    }
    Ok(())
}

fn io(e: lsmkv::Error) -> String {
    e.to_string()
}

fn usage() {
    eprintln!("usage: kv <dir> <command> [args]");
    eprintln!("  put <key> <value>   insert or overwrite");
    eprintln!("  get <key>           read a value");
    eprintln!("  del <key>           delete a key");
    eprintln!("  scan                list all live key=value pairs");
    eprintln!("  flush               force the memtable out to an SSTable");
    eprintln!("  compact             merge all SSTables into one");
    eprintln!("  stats               show store internals");
}
