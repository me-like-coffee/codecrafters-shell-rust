use crate::builtins::is_builtin;
use is_executable::IsExecutable;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context as RlContext, Helper};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;

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

        let mut unique_matches = HashSet::new();

        // Path Completion Logic (Nested Paths)
        if let Some(last_slash_idx) = word.rfind('/') {
            // Split "path/to/f" into "path/to/" and "f"
            let (dir_path_str, prefix) = word.split_at(last_slash_idx + 1);

            // Use "." if the path is empty (though rfind('/') ensures it's not)
            let search_dir = if dir_path_str.is_empty() {
                "."
            } else {
                dir_path_str
            };

            if let Ok(entries) = fs::read_dir(search_dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.starts_with(prefix) {
                            // Construct full path for the replacement: "path/to/" + "file.txt"
                            let mut full_path = dir_path_str.to_string();
                            full_path.push_str(name);
                            unique_matches.insert(full_path);
                        }
                    }
                }
            }
        } else {
            // Fallback: Command and Current Directory Completion (No Slashes)

            // Only check builtins and PATH if we are completing the first word
            if start == 0 {
                for b in &self.builtins {
                    if b.starts_with(word) {
                        unique_matches.insert(b.clone());
                    }
                }

                if let Some(paths) = env::var_os("PATH") {
                    for dir in env::split_paths(&paths) {
                        if let Ok(entries) = fs::read_dir(dir) {
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                    if name.starts_with(word) && path.is_executable() {
                                        unique_matches.insert(name.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Always check the current directory
            if let Ok(entries) = fs::read_dir(".") {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.starts_with(word) {
                            unique_matches.insert(name.to_string());
                        }
                    }
                }
            }
        }

        let mut candidates: Vec<Pair> = unique_matches
            .into_iter()
            .map(|name| Pair {
                display: name.clone(),
                replacement: format!("{} ", name),
            })
            .collect();

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
