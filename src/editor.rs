use crate::shell::Completions;
use is_executable::IsExecutable;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context as RlContext, Helper};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::process::Command;

pub struct ShellHelper {
    pub builtins: Vec<String>,
    pub completions: Completions,
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &RlContext<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let slice = &line[..pos];

        // Identify the start of the word being completed (last space)
        let start = slice.rfind(' ').map(|i| i + 1).unwrap_or(0);
        let word = &slice[start..];

        // Programmable completion: only for arguments, not the command name
        if start > 0 {
            let before = slice[..start].split_whitespace();
            let cmd = before.clone().next().unwrap_or("");
            let prev = before.last().unwrap_or("");

            let script = self.completions.borrow().get(cmd).cloned();
            if let Some(script) = script {
                let output = Command::new(&script).args([cmd, word, prev]).output();
                let mut candidates = Vec::new();
                if let Ok(out) = output {
                    for l in String::from_utf8_lossy(&out.stdout).lines() {
                        let l = l.trim();
                        if !l.is_empty() {
                            candidates.push(Pair {
                                display: l.to_string(),
                                replacement: format!("{} ", l),
                            });
                        }
                    }
                }
                return Ok((start, candidates));
            }
        }

        // Key: Replacement String, Value: Pair object
        let mut unique_matches = HashMap::new();

        // Path and Filename Completion
        let (search_dir, dir_path_str, prefix) = if let Some(last_slash_idx) = word.rfind('/') {
            let (d, p) = word.split_at(last_slash_idx + 1);
            (d, d, p)
        } else {
            (".", "", word)
        };

        if let Ok(entries) = fs::read_dir(search_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str()
                    && name.starts_with(prefix)
                {
                    let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

                    // display: Show directories with a trailing /, files with no trailing char
                    let mut display = name.to_string();
                    // replacement: includes the directory context
                    let mut replacement = dir_path_str.to_string();
                    replacement.push_str(name);

                    if is_dir {
                        display.push('/');
                        replacement.push('/'); // No trailing space for directories
                    } else {
                        replacement.push(' '); // Trailing space for files
                    }

                    unique_matches.insert(
                        replacement.clone(),
                        Pair {
                            display,
                            replacement,
                        },
                    );
                }
            }
        }

        // Command Completion (Only for the first word and if not a path)
        if start == 0 && !word.contains('/') {
            // Check Builtins
            for b in &self.builtins {
                if b.starts_with(word) {
                    unique_matches.insert(
                        format!("{} ", b),
                        Pair {
                            display: b.clone(),
                            replacement: format!("{} ", b),
                        },
                    );
                }
            }

            // Check PATH for executables
            if let Some(paths) = env::var_os("PATH") {
                for dir in env::split_paths(&paths) {
                    if let Ok(entries) = fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                                && name.starts_with(word)
                                && path.is_executable()
                            {
                                unique_matches.insert(
                                    format!("{} ", name),
                                    Pair {
                                        display: name.to_string(),
                                        replacement: format!("{} ", name),
                                    },
                                );
                            }
                        }
                    }
                }
            }
        }

        let mut candidates: Vec<Pair> = unique_matches.into_values().collect();
        // Alpha-sort as required
        candidates.sort_by(|a, b| a.display.cmp(&b.display));

        Ok((start, candidates))
    }
}

impl Hinter for ShellHelper {
    type Hint = String;
    fn hint(&self, _: &str, _: usize, _: &RlContext<'_>) -> Option<String> {
        None
    }
}

impl Highlighter for ShellHelper {}
impl Validator for ShellHelper {}
impl Helper for ShellHelper {}
