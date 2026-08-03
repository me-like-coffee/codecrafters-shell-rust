use crate::builtins::{is_builtin, run_builtin};
use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::process::{ChildStdout, Command, Stdio};

enum InputSource {
    Terminal,
    Pipe(ChildStdout),
    Buffer(Vec<u8>),
}

#[derive(Debug)]
pub struct CommandStage {
    pub tokens: Vec<String>,
    pub stdout_file: Option<File>,
    pub stderr_file: Option<File>,
}

pub fn parse_stage(tokens: Vec<String>) -> Result<CommandStage> {
    let mut clean_args = Vec::new();
    let mut i = 0;
    let mut stdout_file = None;
    let mut stderr_file = None;

    while i < tokens.len() {
        let token = &tokens[i];
        let is_stdout = matches!(token.as_str(), ">" | "1>" | ">>" | "1>>");
        let is_stderr = matches!(token.as_str(), "2>" | "2>>");
        let is_append = matches!(token.as_str(), ">>" | "1>>" | "2>>");

        if (is_stdout || is_stderr)
            && let Some(filename) = tokens.get(i + 1)
        {
            let mut options = OpenOptions::new();
            options.write(true).create(true);
            if is_append {
                options.append(true);
            } else {
                options.truncate(true);
            }
            let file = options.open(filename).context("file open error")?;
            if is_stderr {
                stderr_file = Some(file);
            } else {
                stdout_file = Some(file);
            }
            i += 2;
            continue;
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

fn create_pipe_from_data(data: Vec<u8>) -> Result<Stdio> {
    let mut child = Command::new("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().unwrap();
    std::thread::spawn(move || {
        let _ = stdin.write_all(&data);
    });
    Ok(Stdio::from(child.stdout.take().unwrap()))
}

pub fn execute_pipeline(
    mut stages: Vec<CommandStage>,
    history: &mut Vec<String>,
    sync_idx: &mut usize,
) -> Result<()> {
    let mut prev_input = InputSource::Terminal;
    let mut children = Vec::new();
    let n_stages = stages.len();

println!("{:?}", stages);

    for i in 0..n_stages {
        let mut stage = stages.remove(0);
        if stage.tokens.is_empty() {
            continue;
        }
        let command = stage.tokens.remove(0);
        let is_last = i == n_stages - 1;

        if is_builtin(&command) {
            if let InputSource::Pipe(mut pipe) = prev_input {
                let mut discard = Vec::new();
                let _ = pipe.read_to_end(&mut discard);
            }
            let mut builtin_out = Vec::new();
            let mut std_err = io::stderr();
            let err_writer: &mut dyn Write = match stage.stderr_file.as_mut() {
                Some(f) => f,
                None => &mut std_err,
            };
            run_builtin(
                &command,
                &stage.tokens,
                &mut builtin_out,
                err_writer,
                history,
                sync_idx,
            )?;
            if is_last {
                if let Some(mut f) = stage.stdout_file {
                    f.write_all(&builtin_out)?;
                } else {
                    io::stdout().write_all(&builtin_out)?;
                }
                prev_input = InputSource::Terminal;
            } else {
                prev_input = InputSource::Buffer(builtin_out);
            }
        } else {
            let mut cmd = Command::new(&command);
            cmd.args(&stage.tokens);
            match prev_input {
                InputSource::Terminal => {
                    cmd.stdin(Stdio::inherit());
                }
                InputSource::Pipe(pipe) => {
                    cmd.stdin(Stdio::from(pipe));
                }
                InputSource::Buffer(data) => {
                    cmd.stdin(create_pipe_from_data(data)?);
                }
            }
            if is_last {
                cmd.stdout(
                    stage
                        .stdout_file
                        .map(Stdio::from)
                        .unwrap_or(Stdio::inherit()),
                );
            } else {
                cmd.stdout(Stdio::piped());
            }
            cmd.stderr(
                stage
                    .stderr_file
                    .map(Stdio::from)
                    .unwrap_or(Stdio::inherit()),
            );

            let mut child = cmd
                .spawn()
                .map_err(|_| anyhow::anyhow!("{}: command not found", command))?;
            if !is_last {
                prev_input = InputSource::Pipe(child.stdout.take().unwrap());
            } else {
                prev_input = InputSource::Terminal;
            }
            children.push(child);
        }
    }
    for mut child in children {
        let _ = child.wait();
    }
    Ok(())
}
