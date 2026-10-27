use crate::shell::Shell;
use crate::utils::find_exe_path;
use anyhow::{Context, Result};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub const BUILTINS: &[&str] = &[
    "cd", "declare", "echo", "exit", "history", "jobs", "pwd", "type",
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
            // Poll every job; a job is done when all its children have exited.
            let done: Vec<bool> = shell
                .jobs
                .iter_mut()
                .map(|job| {
                    job.children
                        .iter_mut()
                        .all(|c| matches!(c.try_wait(), Ok(Some(_)) | Err(_)))
                })
                .collect();

            // Print using the same marker logic as before, based on the full list.
            let n = shell.jobs.len();
            for (i, job) in shell.jobs.iter().enumerate() {
                let marker = if i + 1 == n {
                    '+'
                } else if i + 2 == n {
                    '-'
                } else {
                    ' '
                };
                if done[i] {
                    writeln!(out, "[{}]{}  {:<24}{}", job.id, marker, "Done", job.command)?;
                } else {
                    writeln!(
                        out,
                        "[{}]{}  {:<24}{} &",
                        job.id, marker, "Running", job.command
                    )?;
                }
            }

            // Drop the finished jobs so the next `jobs` call doesn't show them.
            let mut i = 0;
            shell.jobs.retain(|_| {
                let keep = !done[i];
                i += 1;
                keep
            });
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
