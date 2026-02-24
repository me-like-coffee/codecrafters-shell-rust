mod parser;

use anyhow::{Context, Result};
use is_executable::IsExecutable;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Context as RlContext, Editor, Helper};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};

// --- Rustyline Helper Implementation ---

struct ShellHelper {
    builtins: Vec<String>,
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &RlContext<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        // We only autocomplete at the start of the command (first word)
        let slice = &line[..pos];

        let mut candidates = Vec::new();
        for builtin in &self.builtins {
            if builtin.starts_with(slice) {
                candidates.push(Pair {
                    display: builtin.to_string(),
                    replacement: format!("{} ", builtin), // Add trailing space
                });
            }
        }

        Ok((0, candidates))
    }
}

impl Hinter for ShellHelper {
    type Hint = String;
    fn hint(&self, _line: &str, _pos: usize, _ctx: &RlContext<'_>) -> Option<String> {
        None
    }
}

impl Highlighter for ShellHelper {}
impl Validator for ShellHelper {}
impl Helper for ShellHelper {}

// --- Shell Logic ---

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
                    .with_context(|| format!("file error: {}", filename))?;
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
        .spawn()?;
    let mut stdin = child.stdin.take().unwrap();
    std::thread::spawn(move || {
        let _ = stdin.write_all(&data);
    });
    Ok(Stdio::from(child.stdout.take().unwrap()))
}

fn execute_pipeline(
    mut stages: Vec<CommandStage>,
    history: &mut Vec<String>,
    sync_idx: &mut usize,
) -> Result<()> {
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
                continue;
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
                let out = child.stdout.take().context("failed to take stdout")?;
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

fn run_builtin(
    cmd: &str,
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
    hist: &mut Vec<String>,
    idx: &mut usize,
) -> Result<()> {
    match cmd {
        "echo" => {
            writeln!(out, "{}", args.join(" "))?;
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
        "history" => {
            let flag = args.first().map(|s| s.as_str());
            match flag {
                Some("-r") => {
                    if let Some(p) = args.get(1) {
                        if let Ok(c) = fs::read_to_string(p) {
                            for l in c.lines() {
                                if !l.trim().is_empty() {
                                    hist.push(l.trim().to_string());
                                }
                            }
                        }
                    }
                }
                Some("-w") => {
                    if let Some(p) = args.get(1) {
                        if let Ok(mut f) = File::create(p) {
                            for e in hist.iter() {
                                writeln!(f, "{}", e)?;
                            }
                            *idx = hist.len();
                        }
                    }
                }
                Some("-a") => {
                    if let Some(p) = args.get(1) {
                        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
                            for e in &hist[*idx..] {
                                writeln!(f, "{}", e)?;
                            }
                            *idx = hist.len();
                        }
                    }
                }
                _ => {
                    let limit = args
                        .first()
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(hist.len());
                    for (i, e) in hist
                        .iter()
                        .enumerate()
                        .skip(hist.len().saturating_sub(limit))
                    {
                        writeln!(out, "{:>5}  {}", i + 1, e)?;
                    }
                }
            }
        }
        "pwd" => {
            writeln!(out, "{}", env::current_dir()?.display())?;
        }
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
        "exit" => {}
        _ => unreachable!(),
    }
    Ok(())
}

fn main() -> Result<()> {
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let helper = ShellHelper {
        builtins: vec!["echo".to_string(), "exit".to_string()],
    };
    let mut rl: Editor<ShellHelper, _> = Editor::with_config(config)?;
    rl.set_helper(Some(helper));

    let mut history_list = Vec::new();
    if let Ok(hf) = env::var("HISTFILE") {
        if let Ok(c) = fs::read_to_string(&hf) {
            for l in c.lines() {
                if !l.trim().is_empty() {
                    let _ = rl.add_history_entry(l.trim());
                    history_list.push(l.trim().to_string());
                }
            }
        }
    }
    let mut history_sync_idx = history_list.len();

    loop {
        let readline = rl.readline("$ ");
        match readline {
            Ok(input) => {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    continue;
                }
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

                if stages.len() == 1 && stages[0].tokens.first().map(|s| s.as_str()) == Some("exit")
                {
                    break;
                }

                if let Err(e) = execute_pipeline(stages, &mut history_list, &mut history_sync_idx) {
                    eprintln!("{}", e);
                }
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    if let Ok(hf) = env::var("HISTFILE") {
        if let Ok(mut f) = File::create(hf) {
            for e in &history_list {
                let _ = writeln!(f, "{}", e);
            }
        }
    }

    Ok(())
}
