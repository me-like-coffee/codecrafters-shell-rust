use crate::shell::Shell;
use crate::utils::find_exe_path;
use anyhow::{Context, Result};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub const BUILTINS: &[&str] = &[
    "cd", "complete", "declare", "echo", "exit", "history", "jobs", "pwd", "type",
];

pub fn is_builtin(cmd: &str) -> bool {
    BUILTINS.contains(&cmd)
}

fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn run_builtin(
    cmd: &str,
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
    shell: &mut Shell,
) -> Result<()> {
    match cmd {
        "cd" => {
            let target = args.first().map(|s| s.as_str()).unwrap_or("~");
            let p = if target == "~" {
                dirs::home_dir().context("home error")?
            } else {
                PathBuf::from(target)
            };
            if env::set_current_dir(&p).is_err() {
                writeln!(err, "cd: {}: No such file or directory", target)?;
            }
        }
        "complete" => match args.first().map(|s| s.as_str()) {
            Some("-C") => {
                if let (Some(script), Some(cmd)) = (args.get(1), args.get(2)) {
                    shell
                        .completions
                        .borrow_mut()
                        .insert(cmd.clone(), script.clone());
                } else {
                    writeln!(err, "complete: -C: option requires an argument")?;
                }
            }
            Some("-p") => {
                if let Some(name) = args.get(1) {
                    match shell.completions.borrow().get(name) {
                        Some(script) => writeln!(out, "complete -C '{}' {}", script, name)?,
                        None => writeln!(out, "complete: {}: no completion specification", name)?,
                    }
                }
            }
            Some("-r") => {
                if let Some(name) = args.get(1) {
                    shell.completions.borrow_mut().remove(name);
                }
            }
            _ => {}
        },
        "declare" => {
            if args.first().map(|s| s.as_str()) == Some("-p") {
                if let Some(name) = args.get(1) {
                    match shell.vars.get(name) {
                        Some(value) => writeln!(out, "declare -- {}=\"{}\"", name, value)?,
                        None => writeln!(out, "declare: {}: not found", name)?,
                    }
                }
            } else {
                for arg in args {
                    let name = arg.split_once('=').map(|(n, _)| n).unwrap_or(arg);
                    if !is_valid_identifier(name) {
                        writeln!(err, "declare: `{}': not a valid identifier", arg)?;
                        continue;
                    }
                    if let Some((name, value)) = arg.split_once('=') {
                        shell.vars.insert(name.to_string(), value.to_string());
                    }
                }
            }
        }
        "echo" => {
            writeln!(out, "{}", args.join(" "))?;
        }
        "history" => {
            let flag = args.first().map(|s| s.as_str());
            match flag {
                Some("-r") => {
                    if let Some(p) = args.get(1)
                        && let Ok(c) = fs::read_to_string(p)
                    {
                        for l in c.lines() {
                            if !l.trim().is_empty() {
                                shell.history.push(l.trim().to_string());
                            }
                        }
                    }
                }
                Some("-w") => {
                    if let Some(p) = args.get(1) {
                        let mut f = File::create(p)?;
                        for e in shell.history.iter() {
                            writeln!(f, "{}", e)?;
                        }
                        shell.sync_idx = shell.history.len();
                    }
                }
                Some("-a") => {
                    if let Some(p) = args.get(1) {
                        let mut f = OpenOptions::new().create(true).append(true).open(p)?;
                        for e in &shell.history[shell.sync_idx..] {
                            writeln!(f, "{}", e)?;
                        }
                        shell.sync_idx = shell.history.len();
                    }
                }
                _ => {
                    let limit = args
                        .first()
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(shell.history.len());
                    for (i, e) in shell
                        .history
                        .iter()
                        .enumerate()
                        .skip(shell.history.len().saturating_sub(limit))
                    {
                        writeln!(out, "{:>5}  {}", i + 1, e)?;
                    }
                }
            }
        }
        "jobs" => {
            for line in shell.job_lines(true) {
                writeln!(out, "{}", line)?;
            }
        }
        "pwd" => {
            writeln!(out, "{}", env::current_dir()?.display())?;
        }
        "type" => match args.first() {
            Some(target) => {
                if is_builtin(target) {
                    writeln!(out, "{} is a shell builtin", target)?;
                } else if let Some(p) = find_exe_path(target) {
                    writeln!(out, "{} is {}", target, p.display())?;
                } else {
                    writeln!(out, "{}: not found", target)?;
                }
            }
            None => writeln!(err, "type: missing argument")?,
        },
        _ => {}
    }
    Ok(())
}
