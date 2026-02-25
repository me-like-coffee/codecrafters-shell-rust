use is_executable::IsExecutable;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context as RlContext, Helper};
use std::collections::HashMap; // Use HashMap instead of HashSet
use std::env;
use std::fs;

pub struct ShellHelper {
    pub builtins: Vec<String>,
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
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(prefix) {
                        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

                        let mut replacement = dir_path_str.to_string();
                        replacement.push_str(name);

                        let display = replacement.clone();

                        if is_dir {
                            replacement.push('/');
                        } else {
                            replacement.push(' ');
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
        }

        // Command Completion (Only for the first word and if not a path)
        if start == 0 && !word.contains('/') {
            // Check Builtins
            for b in &self.builtins {
                if b.starts_with(word) {
                    let replacement = format!("{} ", b);
                    unique_matches.insert(
                        replacement.clone(),
                        Pair {
                            display: b.clone(),
                            replacement,
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
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                if name.starts_with(word) && path.is_executable() {
                                    let replacement = format!("{} ", name);
                                    unique_matches.insert(
                                        replacement.clone(),
                                        Pair {
                                            display: name.to_string(),
                                            replacement,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Convert the HashMap values into a sorted Vec of Pairs
        let mut candidates: Vec<Pair> = unique_matches.into_values().collect();
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
