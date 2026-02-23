mod parser;

use anyhow::{Context, Result};
use is_executable::IsExecutable;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};

fn find_exe_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_executable())
}

fn is_builtin(cmd: &str) -> bool {
    matches!(cmd, "exit" | "echo" | "type" | "pwd" | "cd" | "history")
}

enum InputSource {
    Terminal,
    Pipe(ChildStdout),
    Buffer(Vec<u8>),
}

struct CommandStage {
    tokens: Vec<String>,
    stdout_file: Option<File>,
    stderr_file: Option<File>,
}

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

fn create_pipe_from_data(data: Vec<u8>) -> Result<Stdio> {
    let mut child = Command::new("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to spawn bridge process")?;
    let mut stdin = child.stdin.take().unwrap();
    std::thread::spawn(move || {
        let _ = stdin.write_all(&data);
    });
    Ok(Stdio::from(child.stdout.take().unwrap()))
}

fn run_builtin(
    cmd: &str,
    args: &[String],
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    history: &[String],
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
        "history" => {
            let limit = args
                .first()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(history.len());
            let skip_count = history.len().saturating_sub(limit);
            for (i, entry) in history.iter().enumerate().skip(skip_count) {
                writeln!(stdout, "{:>5}  {}", i + 1, entry)?;
            }
        }
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

fn execute_pipeline(mut stages: Vec<CommandStage>, history: &[String]) -> Result<()> {
    let mut prev_input = InputSource::Terminal;
    let mut children: Vec<Child> = Vec::new();
    let n_stages = stages.len();

    for i in 0..n_stages {
        let mut stage = stages.remove(0);
        if stage.tokens.is_empty() {
            continue;
        }
        let command = stage.tokens.remove(0);
        let is_last = i == n_stages - 1;

        if is_builtin(&command) {
            if command == "exit" {
                std::process::exit(0);
            }
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
                let out = stage
                    .stdout_file
                    .map(Stdio::from)
                    .unwrap_or(Stdio::inherit());
                cmd.stdout(out);
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
                let out = child.stdout.take().context("failed to capture stdout")?;
                prev_input = InputSource::Pipe(out);
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

fn main() -> Result<()> {
    // 1. Initialize the Rustyline Editor
    let mut rl = DefaultEditor::new().context("failed to initialize line editor")?;
    let mut history_list = Vec::new();

    loop {
        // 2. Readline handles Up/Down arrows and returns the final string
        let readline = rl.readline("$ ");

        match readline {
            Ok(input) => {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // 3. Update both Rustyline UI history and our builtin history list
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
                    stages.push(parse_stage(st)?);
                }

                if let Err(e) = execute_pipeline(stages, &history_list) {
                    eprintln!("{}", e);
                }
            }
            // 4. Handle Ctrl+C (Interrupted) and Ctrl+D (EOF/Eof)
            Err(ReadlineError::Interrupted) => {
                continue; // Clear current line on Ctrl+C
            }
            Err(ReadlineError::Eof) => {
                break; // Exit shell on Ctrl+D
            }
            Err(err) => {
                eprintln!("Editor Error: {:?}", err);
                break;
            }
        }
    }
    Ok(())
}
