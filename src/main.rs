use anyhow::{Context, Result};
use is_executable::IsExecutable;
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

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

/// Handles builtin commands. Returns Result to allow for anyhow context.
fn run_builtin(cmd: &str, args: &[&str], writer: &mut dyn Write) -> Result<()> {
    match cmd {
        "echo" => {
            writeln!(writer, "{}", args.join(" "))?;
        }
        "type" => match args.first() {
            Some(&target) => {
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
            let target = args.first().copied().unwrap_or("~");
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

/// Handles external command execution.
fn run_external(cmd: &str, args: &[&str]) -> Result<()> {
    let _ = Command::new(cmd)
        .args(args)
        .spawn()
        .context("failed to spawn command")?
        .wait()
        .context("failed to wait for command")?;

    Ok(())
}

fn main() -> Result<()> {
    loop {
        print!("$ ");
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        let bytes_read = io::stdin()
            .read_line(&mut input)
            .context("failed to read from stdin")?;

        // Exit on EOF (Ctrl+D)
        if bytes_read == 0 {
            break;
        }

        let mut parts = input.split_whitespace();
        let command = match parts.next() {
            Some(cmd) => cmd,
            None => continue,
        };
        let args: Vec<&str> = parts.collect();

        // Handle 'exit' immediately to break the loop
        if command == "exit" {
            break;
        }

        let result = if is_builtin(command) {
            run_builtin(command, &args, &mut io::stdout())
        } else {
            run_external(command, &args)
        };

        // Catch and report recoverable errors
        if let Err(e) = result {
            // Check if it's a "command not found" style error
            if let Some(io_err) = e.downcast_ref::<io::Error>()
                && io_err.kind() == io::ErrorKind::NotFound
            {
                println!("{}: command not found", command);
                continue;
            }
            // Print other errors with their context
            eprintln!("{}: {:?}", command, e);
        }
    }

    Ok(())
}
