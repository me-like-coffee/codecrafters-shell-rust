mod parser;

use anyhow::{Context, Result};
use is_executable::IsExecutable;
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

fn find_exe_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_executable())
}

fn is_builtin(cmd: &str) -> bool {
    matches!(cmd, "exit" | "echo" | "type" | "pwd" | "cd")
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

fn run_external(cmd: &str, args: &[String]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .spawn()
        .context("failed to spawn command")?
        .wait()
        .context("failed to wait for command")?;

    if !status.success() {
        // External command error codes don't stop the shell loop
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

        let mut token_iter = tokens.into_iter();
        let command = token_iter.next().unwrap();
        let args: Vec<String> = token_iter.collect();

        if command == "exit" {
            break;
        }

        let result = if is_builtin(&command) {
            run_builtin(&command, &args, &mut io::stdout())
        } else {
            run_external(&command, &args)
        };

        if let Err(e) = result {
            if let Some(io_err) = e.downcast_ref::<io::Error>()
                && io_err.kind() == io::ErrorKind::NotFound
            {
                println!("{}: command not found", command);
                continue;
            }
            eprintln!("{}: {:?}", command, e);
        }
    }
    Ok(())
}
