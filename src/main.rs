mod parser;

use anyhow::{Context, Result};
use is_executable::IsExecutable;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn find_exe_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_executable())
}

fn is_builtin(cmd: &str) -> bool {
    matches!(cmd, "exit" | "echo" | "type" | "pwd" | "cd")
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

        if stages.len() == 1 {
            match execute_single_stage(stages.remove(0)) {
                Ok(_) => {}
                Err(e) => eprintln!("{}", e),
            }
        } else if stages.len() == 2 {
            execute_two_stage_pipeline(stages.remove(0), stages.remove(0))?;
        }
    }
    Ok(())
}

fn execute_single_stage(mut stage: CommandStage) -> Result<()> {
    if stage.tokens.is_empty() {
        return Ok(());
    }
    let command = stage.tokens.remove(0);

    if command == "exit" {
        std::process::exit(0);
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
        run_builtin(&command, &stage.tokens, out, err)?;
    } else {
        let stdout = stage
            .stdout_file
            .map(Stdio::from)
            .unwrap_or(Stdio::inherit());
        let stderr = stage
            .stderr_file
            .map(Stdio::from)
            .unwrap_or(Stdio::inherit());
        let mut child = Command::new(&command)
            .args(&stage.tokens)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .map_err(|_| anyhow::anyhow!("{}: command not found", command))?;
        child.wait()?;
    }
    Ok(())
}

fn execute_two_stage_pipeline(mut stage1: CommandStage, mut stage2: CommandStage) -> Result<()> {
    let cmd1 = stage1.tokens.remove(0);
    let cmd2 = stage2.tokens.remove(0);

    // If Stage 2 is external, we can pipe directly into it.
    // If Stage 2 is a builtin, we'll need to capture the output of Stage 1.
    if !is_builtin(&cmd2) {
        // Stage 2 is external
        let mut child2_cmd = Command::new(&cmd2);
        child2_cmd
            .args(&stage2.tokens)
            .stdin(Stdio::piped())
            .stdout(
                stage2
                    .stdout_file
                    .map(Stdio::from)
                    .unwrap_or(Stdio::inherit()),
            )
            .stderr(
                stage2
                    .stderr_file
                    .map(Stdio::from)
                    .unwrap_or(Stdio::inherit()),
            );

        let mut child2 = child2_cmd.spawn().context("failed to spawn stage 2")?;
        let mut pipe_to_child2 = child2.stdin.take().context("failed to open pipe")?;

        if is_builtin(&cmd1) {
            // Stage 1 (Builtin) -> Stage 2 (External)
            let mut std_err = io::stderr();
            let err: &mut dyn Write = match stage1.stderr_file.as_mut() {
                Some(f) => f,
                None => &mut std_err,
            };
            run_builtin(&cmd1, &stage1.tokens, &mut pipe_to_child2, err)?;
            drop(pipe_to_child2); // Close pipe so child2 gets EOF
        } else {
            // Stage 1 (External) -> Stage 2 (External)
            let mut child1 = Command::new(&cmd1)
                .args(&stage1.tokens)
                .stdout(Stdio::piped())
                .stderr(
                    stage1
                        .stderr_file
                        .map(Stdio::from)
                        .unwrap_or(Stdio::inherit()),
                )
                .spawn()?;

            let mut output1 = child1.stdout.take().context("failed to take stdout")?;
            io::copy(&mut output1, &mut pipe_to_child2)?;
            drop(pipe_to_child2);
            child1.wait()?;
        }
        child2.wait()?;
    } else {
        // Stage 2 is a builtin (e.g., ls | type exit)
        // Note: 'type' ignores stdin, but we still handle the pipe to be correct.
        let mut output_from_stage1 = Vec::new();

        if is_builtin(&cmd1) {
            let mut std_err = io::stderr();
            let err: &mut dyn Write = match stage1.stderr_file.as_mut() {
                Some(f) => f,
                None => &mut std_err,
            };
            run_builtin(&cmd1, &stage1.tokens, &mut output_from_stage1, err)?;
        } else {
            let mut child1 = Command::new(&cmd1)
                .args(&stage1.tokens)
                .stdout(Stdio::piped())
                .stderr(
                    stage1
                        .stderr_file
                        .map(Stdio::from)
                        .unwrap_or(Stdio::inherit()),
                )
                .spawn()?;
            child1
                .stdout
                .take()
                .unwrap()
                .read_to_end(&mut output_from_stage1)?;
            child1.wait()?;
        }

        let mut std_out = io::stdout();
        let mut std_err = io::stderr();
        let out: &mut dyn Write = match stage2.stdout_file.as_mut() {
            Some(f) => f,
            None => &mut std_out,
        };
        let err: &mut dyn Write = match stage2.stderr_file.as_mut() {
            Some(f) => f,
            None => &mut std_err,
        };

        // Execute Stage 2 Builtin.
        // Note: Builtins currently don't consume 'output_from_stage1'.
        run_builtin(&cmd2, &stage2.tokens, out, err)?;
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
        "echo" => {
            // Join arguments with a single space and add a trailing newline.
            // Do NOT manually replace \n; the tokenizer already handled escaping.
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
