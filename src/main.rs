mod builtins;
mod editor;
mod executor;
mod parser;
mod shell;
mod utils;

use anyhow::Result;
use builtins::BUILTINS;
use editor::ShellHelper;
use rustyline::error::ReadlineError;
use rustyline::{CompletionType, Config, Editor};
use shell::Shell;

use std::env;
use std::fs::{self, File};
use std::io::Write;

fn main() -> Result<()> {
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let helper = ShellHelper {
        builtins: BUILTINS.iter().map(|s| s.to_string()).collect(),
    };
    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(helper));

    let mut shell = Shell::default();
    if let Ok(hf) = env::var("HISTFILE")
        && let Ok(c) = fs::read_to_string(&hf)
    {
        for l in c.lines() {
            if !l.trim().is_empty() {
                let _ = rl.add_history_entry(l.trim());
                shell.history.push(l.trim().to_string());
            }
        }
    }
    shell.sync_idx = shell.history.len();

    loop {
        match rl.readline("$ ") {
            Ok(input) => {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(trimmed);
                shell.history.push(trimmed.to_string());

                let mut tokens = match parser::tokenize(&input) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("{}", e);
                        continue;
                    }
                };

                // Detect and strip a trailing `&` (background execution)
                let background = tokens.last().map(|s| s.as_str()) == Some("&");
                if background {
                    tokens.pop();
                }

                // Split tokens into pipeline stages on `|`
                let mut stages_tokens = Vec::new();
                let mut current_chunk = Vec::new();
                for token in tokens {
                    if token == "|" {
                        stages_tokens.push(std::mem::take(&mut current_chunk));
                    } else {
                        current_chunk.push(token);
                    }
                }
                stages_tokens.push(current_chunk);

                // Abort the whole line if any stage fails to parse
                let stages: Result<Vec<_>> = stages_tokens
                    .into_iter()
                    .map(executor::parse_stage)
                    .collect();
                let stages = match stages {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("{e}");
                        continue;
                    }
                };

                if stages.len() == 1 && stages[0].tokens.first().map(|s| s.as_str()) == Some("exit")
                {
                    break;
                }

                // Original command text without the trailing `&`, for the jobs listing
                let text = trimmed.trim_end_matches('&').trim_end();
                if let Err(e) = executor::execute_pipeline(stages, &mut shell, background, text) {
                    eprintln!("{}", e);
                }
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    if let Ok(hf) = env::var("HISTFILE")
        && let Ok(mut f) = File::create(hf)
    {
        for e in &shell.history {
            let _ = writeln!(f, "{}", e);
        }
    }
    Ok(())
}
