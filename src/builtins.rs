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
            _ => {}
        },
        "declare" => {}
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
