mod parser;

use anyhow::{Context, Result};
use is_executable::IsExecutable;
use std::env;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Searches the system PATH for an executable.
fn find_exe_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_executable())
}

fn is_builtin(cmd: &str) -> bool {
    matches!(cmd, "exit" | "echo" | "type" | "pwd" | "cd")
}

/// Helper to identify and extract redirection from tokens.
/// Returns (remaining_args, optional_file_handle)
fn process_redirection(tokens: Vec<String>) -> Result<(Vec<String>, Option<File>)> {
    let mut clean_args = Vec::new();
    let mut i = 0;
    let mut file_handle = None;

    while i < tokens.len() {
        if tokens[i] == ">" || tokens[i] == "1>" {
            if let Some(filename) = tokens.get(i + 1) {
                // Open file: Create if missing, truncate if exists.
                let file = File::create(filename)
                    .with_context(|| format!("failed to create file: {}", filename))?;
                file_handle = Some(file);
                i += 2; // Skip operator and filename
                continue;
            }
        }
        clean_args.push(tokens[i].clone());
        i += 1;
    }

    Ok((clean_args, file_handle))
}

fn run_builtin(cmd: &str, args: &[String], writer: &mut dyn Write) -> Result<()> {
    match cmd {
        "echo" => {
            writeln!(writer, "{}", args.join(" "))?;
        }
        "type" => match args.first() {
            Some(target) => {
                if is_builtin(target) {
                    writeln!(writer, "{} is a shell builtin", target)?;
                } else if let Some(exe_path) = find_exe_path(target) {
                    writeln!(writer, "{} is {}", target, exe_path.display())?;
                } else {
                    writeln!(writer, "{}: not found", target)?;
                }
            }
            None => writeln!(writer, "type: missing argument")?,
        },
        "pwd" => {
            let path = env::current_dir().context("failed to get current directory")?;
            writeln!(writer, "{}", path.display())?;
        }
        "cd" => {
            let target = args.first().map(|s| s.as_str()).unwrap_or("~");
            let target_path = if target == "~" {
                dirs::home_dir().context("could not find home directory")?
            } else {
                PathBuf::from(target)
            };
            if env::set_current_dir(&target_path).is_err() {
                writeln!(writer, "cd: {}: No such file or directory", target)?;
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn run_external(cmd: &str, args: &[String], stdout_file: Option<File>) -> Result<()> {
    let mut command = Command::new(cmd);
    command.args(args);

    // Redirect stdout if a file was provided
    if let Some(file) = stdout_file {
        command.stdout(Stdio::from(file));
    }

    let status = command
        .spawn()
        .context("failed to spawn command")?
        .wait()
        .context("failed to wait for command")?;

    if !status.success() {
        // Child exit code does not affect shell loop
    }
    Ok(())
}

fn main() -> Result<()> {
    loop {
        print!("$ ");
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        if io::stdin().read_line(&mut input).context("failed to read from stdin")? == 0 {
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

        // Handle redirection BEFORE identifying the command
        let (mut clean_tokens, stdout_file) = match process_redirection(tokens) {
            Ok(res) => res,
            Err(e) => {
                eprintln!("{}", e);
                continue;
            }
        };

        if clean_tokens.is_empty() {
            continue;
        }

        let command = clean_tokens.remove(0);
        let args = clean_tokens;

        if command == "exit" {
            break;
        }

        let result = if is_builtin(&command) {
            // For builtins, if we have a file, use it as the writer. 
            // Otherwise, use stdout.
            if let Some(mut file) = stdout_file {
                run_builtin(&command, &args, &mut file)
            } else {
                run_builtin(&command, &args, &mut io::stdout())
            }
        } else {
            run_external(&command, &args, stdout_file)
        };

        if let Err(e) = result {
            if let Some(io_err) = e.downcast_ref::<io::Error>() && io_err.kind() == io::ErrorKind::NotFound {
                    println!("{}: command not found", command);
                    continue;
            }
            eprintln!("{}: {:?}", command, e);
        }
    }
    Ok(())
}