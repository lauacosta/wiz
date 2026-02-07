use std::{
    io::Write as _,
    path::PathBuf,
    process::{Command, Stdio},
    str,
    sync::{
        OnceLock,
        mpsc::{self, Sender},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::Connection;

static SPINNER_TX: OnceLock<Sender<SpinnerMsg>> = OnceLock::new();

fn main() {
    let mut llm = Command::new("llm");
    llm.args([
        "-m",
        "openrouter/anthropic/claude-sonnet-4.5",
        "--no-stream",
        "--log",
    ]);

    let mut args = std::env::args().skip(1);
    let command = args.next();

    match command.as_deref() {
        None => {
            eprintln!("A prompt is required.");
            std::process::exit(1);
        }
        Some("help" | "-h") => {
            print_help();
        }

        Some("spell") => {
            let subcommand = args.next();
            match subcommand.as_deref() {
                Some("status") => check_db_status("spell.db"),
                Some(file) => spell_check(&PathBuf::from(file), llm),
                None => {
                    eprintln!("A file path is required.");
                    std::process::exit(1);
                }
            }
        }
        Some("cmd") => {
            let subcommand = args.next();
            match subcommand.as_deref() {
                Some("list") => {
                    let n = args.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(5);
                    list_cmds(n);
                }
                Some("status") => check_db_status("cmd.db"),
                Some(first_arg) => {
                    let prompt = std::iter::once(first_arg.to_string())
                        .chain(args)
                        .collect::<Vec<_>>();

                    give_cmd(&prompt, llm);
                }
                None => {
                    eprintln!("A prompt is required.");
                    std::process::exit(1);
                }
            }
        }
        Some(first_arg) => {
            let prompt = std::iter::once(first_arg.to_string())
                .chain(args)
                .collect::<Vec<_>>();
            give_cmd(&prompt, llm);
        }
    }
}

fn print_help() {
    eprintln!(
        "\
Usage:
  wiz cmd <text>             Run a one-shot prompt
  wiz cmd list [number]      List the last <number> requests (default: 5)
  wiz spell <file>           Check spelling in a file
  wiz <text>                 Same as wiz cmd <text>
  wiz (spell|cmd) status     Show status of database logging
  wiz help                   Show this help message

Options:
  -h, --help                 Show this help
"
    );
}

fn spell_check(file: &PathBuf, mut cmd: Command) {
    let content = std::fs::read_to_string(file).unwrap_or_else(|_| {
        eprintln!("Failed to read file: {}", file.as_path().display());
        std::process::exit(1);
    });
    init_spinner();

    let system_prompt = "#
You are a professional editor. Please identify typos and grammatical  errors in the following blog post. 

IMPORTANT RULES:    
    1. Find only typos and grammatical errors
    2. Do NOT suggest style changes or voice modifications
    3. Do NOT suggest adding or removing content
    4. For each error found, provide the exact text to replace and what to replace it with

    Please respond in this exact format: 
        REPLACEMENTS_START 
            replace 'incorrect text 1' with 'correct text 1' 
            replace 'incorrect text 2' with 'correct text 2'  
        REPLACEMENTS_END 

        SUGGESTIONS_START 
            - [optional style/clarity suggestion 1] 
            - [optional style/clarity suggestion 2] 
            - [optional style/clarity suggestion 3]
        SUGGESTIONS_END

Here is the text to check: #";

    let db_path = get_data_dir().join("spell.db");
    let mut child = cmd
        .args(["-d", &db_path.display().to_string(), "-s", system_prompt])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn llm");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(content.as_bytes())
            .expect("Failed to write to stdin");
    }

    let output = child.wait_with_output().expect("Failed to read output");
    stop_spinner();

    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }

    let llm_output = String::from_utf8_lossy(&output.stdout);

    let mut replacements: Vec<(String, String)> = vec![];

    if let Some(start) = llm_output.find("REPLACEMENTS_START")
        && let Some(end) = llm_output.find("REPLACEMENTS_END")
    {
        let block = &llm_output[start + "REPLACEMENTS_START".len()..end];

        for line in block.lines() {
            let line = line.trim();
            if let Some((old_text, new_text)) = parse_replacement_line(line)
                && old_text != new_text
            {
                replacements.push((old_text, new_text));
            }
        }
    }

    let mut suggestions: Vec<String> = vec![];
    if let Some(start) = llm_output.find("SUGGESTIONS_START")
        && let Some(end) = llm_output.find("SUGGESTIONS_END")
    {
        let block = &llm_output[start + "SUGGESTIONS_START".len()..end];

        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('-') {
                suggestions.push(trimmed.trim_start_matches('-').trim().to_string());
            }
        }
    }

    if replacements.is_empty() && suggestions.is_empty() {
        println!("No typos or grammar issues found.");
        return;
    }

    if !replacements.is_empty() {
        println!("\nProposed Replacements:");

        for (idx, (old, new)) in replacements.iter().enumerate() {
            let idx = idx + 1;
            if content.contains(old) {
                println!("{idx}. '{old}' → '{new}'");
            } else {
                println!("{idx}. Hallucination: '{old}' → '{new}'");
            }
        }
    }

    if !suggestions.is_empty() {
        println!("\nSuggestions:");
        for s in suggestions {
            println!("- {s}");
        }
    }
}

fn parse_replacement_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();

    if !line.starts_with("replace") {
        return None;
    }

    let after_replace = line.strip_prefix("replace")?.trim();

    let first_quote = after_replace.find('\'')?;
    let after_first_quote = &after_replace[first_quote + 1..];

    let second_quote = after_first_quote.find('\'')?;
    let old_text = &after_first_quote[..second_quote];

    let after_old = &after_first_quote[second_quote + 1..].trim();

    let after_with = after_old.strip_prefix("with")?.trim();

    let third_quote = after_with.find('\'')?;
    let after_third_quote = &after_with[third_quote + 1..];

    let fourth_quote = after_third_quote.find('\'')?;
    let new_text = &after_third_quote[..fourth_quote];

    Some((old_text.to_string(), new_text.to_string()))
}

fn give_cmd(user_prompt: &[String], mut cmd: Command) {
    if user_prompt.is_empty() {
        eprintln!("A prompt is required.");
        std::process::exit(1);
    }
    let user_prompt = user_prompt.join(" ");

    init_spinner();

    let system_prompt = "#
You are a command generator.

Return exactly ONE fish shell command.
Do not include explanations.
Do not include backticks.
Do not include markdown.
Do not include extra lines.
Do not include commentary.
Output must be a single line of plain text.

If the request is dangerous, output exactly:
REFUSE

Nothing else.
#";

    let db_path = get_data_dir().join("cmd.db");
    let output = cmd
        .args([
            "-d",
            &db_path.display().to_string(),
            "-s",
            system_prompt,
            &user_prompt,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("I expected an output");

    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }
    stop_spinner();

    let result = String::from_utf8_lossy(&output.stdout);

    println!("{}", result.trim());
}

fn list_cmds(number: u32) {
    let db_path = get_data_dir().join("cmd.db");

    if !db_path.exists() {
        eprintln!("Path: {} does not exist.", db_path.display());
        std::process::exit(1);
    }

    let conn = rusqlite::Connection::open(db_path)
        .expect("I expect to be able to open a connection to the sqlite db");
    let sql_str = format!(
        "select prompt, response, token_details, model from responses order by datetime_utc desc limit {number}"
    );
    let mut stmt = conn
        .prepare(&sql_str)
        .expect("I expect this to be valid SQL");
    let mut rows = stmt.query([]).unwrap();

    while let Some(row) = rows.next().unwrap() {
        let prompt = row.get_ref_unwrap(0).as_str().unwrap();
        let response = row.get_ref_unwrap(1).as_str().unwrap();
        let token_details = row
            .get_ref_unwrap(2)
            .as_str()
            .expect("Failed to interpret as str");

        let model = row
            .get_ref_unwrap(3)
            .as_str()
            .expect("Failed to interpret as str");

        let cost = extract_cost(token_details).unwrap_or_default();

        writeln!(
            std::io::stdout(),
            "\n\x1b[1;32mPrompt:\x1b[0m \"{prompt}\"\n\x1b[1;33mModel:\x1b[0m \"{model}\"\n\x1b[1;33mResponse:\x1b[0m \"{response}\"\n\x1b[1;33mCost:\x1b[0m ${cost:.6} (USD)\n",
        )
        .expect("Failed to write to stdout");
    }
}

fn extract_cost(token_details: &str) -> Option<f64> {
    let key = "\"cost\":";
    let start = token_details.find(key)? + key.len();

    let rest = &token_details[start..];

    let end = rest.find([',', '}']).unwrap_or(rest.len());

    let number_str = &rest[..end].trim();
    number_str.parse::<f64>().ok()
}

enum SpinnerMsg {
    Stop,
}

fn init_spinner() {
    let (tx, rx) = mpsc::channel();

    if SPINNER_TX.set(tx).is_err() {
        return;
    }

    std::thread::spawn(move || {
        let phrases = [
            "Winging it...",
            "Markov-chaining my way to it...",
            "Compressing the internet...",
        ];

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Expected to be able to determine UNIX_EPOCH")
            .subsec_nanos() as usize;

        let msg = phrases[nanos % phrases.len()];

        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let mut idx = 0;

        loop {
            match rx.try_recv() {
                Ok(SpinnerMsg::Stop) => {
                    print!("\r\x1b[K");
                    break;
                }
                Err(mpsc::TryRecvError::Disconnected) => break,
                _ => {}
            }

            print!("\r{} {msg}", frames[idx]);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();

            idx = (idx + 1) % frames.len();
            thread::sleep(Duration::from_millis(80));
        }
    });
}

fn stop_spinner() {
    if let Some(tx) = SPINNER_TX.get() {
        let _ = tx.send(SpinnerMsg::Stop);
    }
    thread::sleep(Duration::from_millis(100));
}

fn get_data_dir() -> PathBuf {
    #[cfg(not(windows))]
    {
        let base = match std::env::var("XDG_DATA_HOME") {
            Ok(xdg) if !xdg.is_empty() => PathBuf::from(xdg),
            _ => {
                let home = std::env::var("HOME").expect("HOME not set");
                PathBuf::from(home).join(".local/share")
            }
        };

        let wiz_dir = base.join("wiz");
        std::fs::create_dir_all(&wiz_dir).expect("Failed to create data directory");
        wiz_dir
    }

    #[cfg(windows)]
    {
        let app_data = std::env::var("APPDATA").expect("APPDATA not set");
        let wiz_dir = PathBuf::from(app_data).join("wiz");
        std::fs::create_dir_all(&wiz_dir).expect("Failed to create data directory");
        wiz_dir
    }
}

fn check_db_status(path: &str) {
    let db_path = get_data_dir().join(path);
    if !db_path.exists() {
        eprintln!("Path: {} does not exist.", db_path.display());
        std::process::exit(1);
    }
    let conn = Connection::open(&db_path).expect("Failed to create connection to db");

    let conversations: i64 = conn
        .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
        .unwrap_or(0);

    let responses: i64 = conn
        .query_row("SELECT COUNT(*) FROM responses", [], |row| row.get(0))
        .unwrap_or(0);

    let metadata = std::fs::metadata(&db_path).unwrap();
    let size_bytes = metadata.len() as f64;
    let size_kb = size_bytes / 1024.0;

    println!("Found log database at {}", db_path.display());
    println!("Number of conversations logged: {conversations}");
    println!("Number of responses logged:     {responses}");
    println!("Database file size:             {size_kb:.2}KB");
}
