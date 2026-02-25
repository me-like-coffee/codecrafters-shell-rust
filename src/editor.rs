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

        // Identify the start of the word being completed
        // If there's a space, we are completing an argument.
        // If not, we are completing the command itself.
        let start = slice.rfind(' ').map(|i| i + 1).unwrap_or(0);
        let prefix = &slice[start..];

        let mut unique_matches = HashSet::new();

        // Command Completion (only if we are at the first word)
        if start == 0 {
            // Check Builtins
            for b in &self.builtins {
                if b.starts_with(prefix) {
                    unique_matches.insert(b.clone());
                }
            }

            // Check PATH
            if let Some(paths) = env::var_os("PATH") {
                for dir in env::split_paths(&paths) {
                    if let Ok(entries) = fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                if name.starts_with(prefix) && path.is_executable() {
                                    unique_matches.insert(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Filename Completion (Always check current directory for arguments or commands)
        // Per requirements: search current directory for files starting with prefix
        if let Ok(entries) = fs::read_dir(".") {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(prefix) {
                        unique_matches.insert(name.to_string());
                    }
                }
            }
        }

        let mut candidates: Vec<Pair> = unique_matches
            .into_iter()
            .map(|name| Pair {
                display: name.clone(),
                replacement: format!("{} ", name), // Add trailing space
            })
            .collect();

        candidates.sort_by(|a, b| a.display.cmp(&b.display));

        // Return 'start' so rustyline knows where to begin the replacement
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
