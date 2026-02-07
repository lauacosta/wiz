use std::{
    io::Write as _,
    path::PathBuf,
    process::{Command, Stdio},
};

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct WizCli {
    #[command(subcommand)]
    pub command: Option<WizCmd>,

    /// Explictly say if you want to log the prompt
    #[arg(long, default_value = "true")]
    pub log: bool,

    /// Fallback prompt if no subcommand is provided
    pub prompt: Vec<String>,
}

#[derive(Subcommand, Clone, Debug, PartialEq, Eq)]
pub enum WizCmd {
    // Reads the file to find suggestions or replacements in the text.
    Spell { file: PathBuf },
    // Gives a one-shot command for what you want.
    Cmd { prompt: Vec<String> },
}

fn main() {
    let args = WizCli::parse();

    let mut llm = Command::new("llm");

    if args.log {
        llm.arg("--log");
    } else {
        llm.arg("--no-log");
    }
    llm.args(&["-m", "openrouter/openai/gpt-4o-mini", "--no-stream"]);

    match args.command {
        Some(WizCmd::Cmd { prompt }) => give_cmd(prompt, llm),
        Some(WizCmd::Spell { file }) => spell_check(file, llm),
        None => {
            if args.prompt.is_empty() {
                eprintln!("A prompt is required.");
                std::process::exit(1);
            }
            give_cmd(args.prompt, llm)
        }
    };
}

fn spell_check(file: PathBuf, mut cmd: Command) {
    let content = std::fs::read_to_string(&file).unwrap_or_else(|_| {
        eprintln!("Failed to read file: {}", file.as_path().display());
        std::process::exit(1);
    });

    let system_prompt = "#You are a professional editor. Please identify typos and grammatical 
        errors in the following blog post. 

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
        Here is the blog post to check: #";

    let mut child = cmd
        .args(&["-s", &system_prompt])
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

    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }

    let llm_output = String::from_utf8_lossy(&output.stdout);

    let replacement_regex = regex::Regex::new(r#"replace\s+'([^']+)'\s+with\s+'([^']*)'"#).unwrap();

    let mut replacements: Vec<(String, String)> = vec![];

    if let Some(start) = llm_output.find("REPLACEMENTS_START") {
        if let Some(end) = llm_output.find("REPLACEMENTS_END") {
            let block = &llm_output[start + "REPLACEMENTS_START".len()..end];

            for line in block.lines() {
                let line = line.trim();
                if let Some(captures) = replacement_regex.captures(line) {
                    let old_text = captures.get(1).unwrap().as_str().to_string();
                    let new_text = captures.get(2).unwrap().as_str().to_string();

                    if old_text != new_text {
                        replacements.push((old_text, new_text));
                    }
                }
            }
        }
    }

    let mut suggestions: Vec<String> = vec![];
    if let Some(start) = llm_output.find("SUGGESTIONS_START") {
        if let Some(end) = llm_output.find("SUGGESTIONS_END") {
            let block = &llm_output[start + "SUGGESTIONS_START".len()..end];

            for line in block.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('-') {
                    suggestions.push(trimmed.trim_start_matches('-').trim().to_string());
                }
            }
        }
    }

    if replacements.is_empty() && suggestions.is_empty() {
        println!("No typos or grammar issues found.");
        return;
    }

    if !replacements.is_empty() {
        println!("\nProposed Replacements:");

        for (old, new) in &replacements {
            if content.contains(old) {
                println!(" '{}' → '{}'", old, new);
            } else {
                println!("Hallucination: '{}' → '{}'", old, new);
            }
        }
    }

    if !suggestions.is_empty() {
        println!("\nSuggestions:");
        for s in suggestions {
            println!("- {}", s);
        }
    }
}

fn give_cmd(user_prompt: Vec<String>, mut cmd: Command) {
    if user_prompt.is_empty() {
        eprintln!("A prompt is required.");
        std::process::exit(1);
    }
    let user_prompt = user_prompt.join(" ");

    // let system_prompt = "#Return a one-shot command for fish, without any backticks or markup
    //     language or explanation. If the request is dangerous tell me.#";
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

    let output = cmd
        .args(&["-s", &system_prompt, &user_prompt])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("I expected an output");

    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }

    let result = String::from_utf8_lossy(&output.stdout);
    println!("{}", result.trim());
}
