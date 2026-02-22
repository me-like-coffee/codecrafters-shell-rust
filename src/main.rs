mod parser;

use anyhow::{Context, Result};
use is_executable::IsExecutable;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

fn find_exe_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_executable())
}

fn is_builtin(cmd: &str) -> bool {
    matches!(cmd, "exit" | "echo" | "type" | "pwd" | "cd")
}

/// Represents a single command within a pipeline, including its redirections.
struct CommandStage {
    tokens: Vec<String>,
    stdout_file: Option<File>,
    stderr_file: Option<File>,
}

/// Parses a stage's tokens to extract redirections.
fn parse_stage(tokens: Vec<String>) -> Result<CommandStage> {
    let mut clean_args = Vec::new();
    let mut i = 0;
    let mut stdout_file = None;
    let mut stderr_file = None;

    while i < tokens.len() {
        let token = &tokens[i];
        let is_stdout = matches!(token.as_str(), ">" | "1>" | ">>" | "1>>");
        let is_stderr = matches!(token.as_str(), "2>" | "2>>");
        let is_append = matches!(token.as_str(), ">>" | "1>>" | "2>>");

        if is_stdout || is_stderr {
            if let Some(filename) = tokens.get(i + 1) {
                let mut options = OpenOptions::new();
                options.write(true).create(true);
                if is_append {
                    options.append(true);
                } else {
                    options.truncate(true);
                }

                let file = options
                    .open(filename)
                    .with_context(|| format!("failed to open file: {}", filename))?;

                if is_stderr {
                    stderr_file = Some(file);
                } else {
                    stdout_file = Some(file);
                }
                i += 2;
                continue;
            }
        }
        clean_args.push(tokens[i].clone());
        i += 1;
    }

    Ok(CommandStage {
        tokens: clean_args,
        stdout_file,
        stderr_file,
    })
}

/// Spawns an external command as part of a pipeline.
fn spawn_external(args: Vec<String>, stdin: Stdio, stdout: Stdio, stderr: Stdio) -> Result<Child> {
    let mut token_iter = args.into_iter();
    let command = token_iter.next().context("empty command stage")?;
    let args: Vec<String> = token_iter.collect();

    Command::new(command)
        .args(args)
        .stdin(stdin)
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .context("failed to spawn command")
}

fn main() -> Result<()> {
    loop {
        print!("$ ");
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        if io::stdin()
            .read_line(&mut input)
            .context("failed to read from stdin")?
            == 0
        {
            break;
        }

        let tokens = match parser::tokenize(&input) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{}", e);
                continue;
            }
        };
        if tokens.is_empty() {
            continue;
        }

        // 1. Split tokens by the pipe operator '|'
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

        // 2. Process redirections for each stage
        let mut stages = Vec::new();
        for st in stages_tokens {
            stages.push(parse_stage(st)?);
        }

        // 3. Execution Logic
        if stages.len() == 1 {
            // Single command logic (Builtin support)
            let mut stage = stages.remove(0);
            if stage.tokens.is_empty() {
                continue;
            }
            let command = stage.tokens.remove(0);

            if command == "exit" {
                break;
            }

            if is_builtin(&command) {
                let mut std_out = io::stdout();
                let mut std_err = io::stderr();
                let out: &mut dyn Write = match stage.stdout_file.as_mut() {
                    Some(f) => f,
                    None => &mut std_out,
                };
                let err: &mut dyn Write = match stage.stderr_file.as_mut() {
                    Some(f) => f,
                    None => &mut std_err,
                };
                if let Err(e) = run_builtin(&command, &stage.tokens, out, err) {
                    eprintln!("{}: {:?}", command, e);
                }
            } else {
                let stdout = stage
                    .stdout_file
                    .map(Stdio::from)
                    .unwrap_or(Stdio::inherit());
                let stderr = stage
                    .stderr_file
                    .map(Stdio::from)
                    .unwrap_or(Stdio::inherit());
                match spawn_external(
                    std::iter::once(command.clone())
                        .chain(stage.tokens)
                        .collect(),
                    Stdio::inherit(),
                    stdout,
                    stderr,
                ) {
                    Ok(mut child) => {
                        child.wait().ok();
                    }
                    Err(_) => println!("{}: command not found", command),
                }
            }
        } else if stages.len() == 2 {
            // Two-stage pipeline (External commands only for now)
            let stage1 = stages.remove(0);
            let stage2 = stages.remove(0);

            // Spawn Stage 1
            let stdout1 = stage1
                .stdout_file
                .map(Stdio::from)
                .unwrap_or(Stdio::piped());
            let stderr1 = stage1
                .stderr_file
                .map(Stdio::from)
                .unwrap_or(Stdio::inherit());

            let mut child1 = match spawn_external(stage1.tokens, Stdio::inherit(), stdout1, stderr1)
            {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("pipeline error: {}", e);
                    continue;
                }
            };

            // Take stdout of child1 to pass to child2
            let pipe_in = child1.stdout.take().context("failed to open pipe")?;

            // Spawn Stage 2
            let stdin2 = Stdio::from(pipe_in);
            let stdout2 = stage2
                .stdout_file
                .map(Stdio::from)
                .unwrap_or(Stdio::inherit());
            let stderr2 = stage2
                .stderr_file
                .map(Stdio::from)
                .unwrap_or(Stdio::inherit());

            let mut child2 = match spawn_external(stage2.tokens, stdin2, stdout2, stderr2) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("pipeline error: {}", e);
                    child1.kill().ok();
                    continue;
                }
            };

            // Wait for both to complete
            child1.wait().ok();
            child2.wait().ok();
        }
    }
    Ok(())
}

fn run_builtin(
    cmd: &str,
    args: &[String],
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<()> {
    match cmd {
        "echo" => writeln!(stdout, "{}", args.join(" "))?,
        "type" => match args.first() {
            Some(target) => {
                if is_builtin(target) {
                    writeln!(stdout, "{} is a shell builtin", target)?;
                } else if let Some(exe_path) = find_exe_path(target) {
                    writeln!(stdout, "{} is {}", target, exe_path.display())?;
                } else {
                    writeln!(stdout, "{}: not found", target)?;
                }
            }
            None => writeln!(stderr, "type: missing argument")?,
        },
        "pwd" => {
            let path = env::current_dir().context("failed to get current directory")?;
            writeln!(stdout, "{}", path.display())?;
        }
        "cd" => {
            let target = args.first().map(|s| s.as_str()).unwrap_or("~");
            let target_path = if target == "~" {
                dirs::home_dir().context("home dir error")?
            } else {
                PathBuf::from(target)
            };
            if env::set_current_dir(&target_path).is_err() {
                writeln!(stderr, "cd: {}: No such file or directory", target)?;
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
