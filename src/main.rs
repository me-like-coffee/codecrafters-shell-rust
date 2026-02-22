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

/// Checks if a command is a known shell builtin.
fn is_builtin(cmd: &str) -> bool {
    matches!(cmd, "exit" | "echo" | "type" | "pwd" | "cd")
}

/// Extracts redirection operators for stdout (1>) and stderr (2>) from tokens.
/// Returns (cleaned_args, stdout_file, stderr_file).
fn process_redirection(tokens: Vec<String>) -> Result<(Vec<String>, Option<File>, Option<File>)> {
    let mut clean_args = Vec::new();
    let mut i = 0;
    let mut stdout_file = None;
    let mut stderr_file = None;

    while i < tokens.len() {
        let token = &tokens[i];
        if token == ">" || token == "1>" || token == "2>" {
            if let Some(filename) = tokens.get(i + 1) {
                // File::create truncates existing files and creates new ones
                let file = File::create(filename)
                    .with_context(|| format!("failed to create file: {}", filename))?;

                if token == "2>" {
                    stderr_file = Some(file);
                } else {
                    stdout_file = Some(file);
                }

                i += 2; // Skip operator and filename
                continue;
            }
        }
        clean_args.push(tokens[i].clone());
        i += 1;
    }

    Ok((clean_args, stdout_file, stderr_file))
}

/// Handles builtin commands with redirected writers.
fn run_builtin(
    cmd: &str,
    args: &[String],
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<()> {
    match cmd {
        "echo" => {
            writeln!(stdout, "{}", args.join(" "))?;
        }
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
                dirs::home_dir().context("could not find home directory")?
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

/// Handles external command execution with redirected stdio.
fn run_external(
    cmd: &str,
    args: &[String],
    stdout_file: Option<File>,
    stderr_file: Option<File>,
) -> Result<()> {
    let mut command = Command::new(cmd);
    command.args(args);

    if let Some(file) = stdout_file {
        command.stdout(Stdio::from(file));
    }
    if let Some(file) = stderr_file {
        command.stderr(Stdio::from(file));
    }

    let status = command
        .spawn()
        .context("failed to spawn command")?
        .wait()
        .context("failed to wait for command")?;

    if !status.success() {
        // Child exit code is ignored by the shell REPL
    }
    Ok(())
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
            // Exit on EOF (Ctrl+D)
            break;
        }

        // Tokenize input using modular nom parser
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

        // Process redirection operators
        let (mut clean_tokens, mut stdout_file, mut stderr_file) = match process_redirection(tokens) {
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

        // Dispatch to Builtin or External command
        let result = if is_builtin(&command) {
            // Shadowing standard streams to facilitate redirection
            let mut std_out = io::stdout();
            let mut std_err = io::stderr();

            let out_writer: &mut dyn Write = match stdout_file.as_mut() {
                Some(f) => &mut *f,
                None => &mut std_out,
            };
            let err_writer: &mut dyn Write = match stderr_file.as_mut() {
                Some(f) => &mut *f,
                None => &mut std_err,
            };

            run_builtin(&command, &args, out_writer, err_writer)
        } else {
            run_external(&command, &args, stdout_file, stderr_file)
        };

        // 4. Handle Execution Errors
        if let Err(e) = result {
            // Check specifically for "Not Found" to print standard shell error
            if let Some(io_err) = e.downcast_ref::<io::Error>() {
                if io_err.kind() == io::ErrorKind::NotFound {
                    println!("{}: command not found", command);
                    continue;
                }
            }
            // Report all other fatal/contextual errors
            eprintln!("{}: {:?}", command, e);
        }
    }

    Ok(())
}