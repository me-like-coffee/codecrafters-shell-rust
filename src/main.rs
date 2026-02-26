mod builtins;
mod editor;
mod executor;
mod parser;
mod utils;

use anyhow::Result;
use editor::ShellHelper;
use rustyline::error::ReadlineError;
use rustyline::{CompletionType, Config, Editor};
use std::env;
use std::fs::{self, File};
use std::io::Write;

fn main() -> Result<()> {
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let helper = ShellHelper {
        builtins: vec![
            "echo".into(),
            "exit".into(),
            "type".into(),
            "pwd".into(),
            "cd".into(),
            "history".into(),
        ],
    };
    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(helper));

    let mut history_list = Vec::new();
    if let Ok(hf) = env::var("HISTFILE")
        && let Ok(c) = fs::read_to_string(&hf)
    {
        for l in c.lines() {
            if !l.trim().is_empty() {
                let _ = rl.add_history_entry(l.trim());
                history_list.push(l.trim().to_string());
            }
        }
    }
    let mut history_sync_idx = history_list.len();

    loop {
        match rl.readline("$ ") {
            Ok(input) => {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(trimmed);
                history_list.push(trimmed.to_string());

                let tokens = match parser::tokenize(&input) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("{}", e);
                        continue;
                    }
                };

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

                let mut stages = Vec::new();
                for st in stages_tokens {
                    stages.push(executor::parse_stage(st)?);
                }

                if stages.len() == 1 && stages[0].tokens.first().map(|s| s.as_str()) == Some("exit")
                {
                    break;
                }

                if let Err(e) =
                    executor::execute_pipeline(stages, &mut history_list, &mut history_sync_idx)
                {
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
        for e in &history_list {
            let _ = writeln!(f, "{}", e);
        }
    }
    Ok(())
}
