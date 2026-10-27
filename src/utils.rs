use is_executable::IsExecutable;
use std::env;
use std::path::PathBuf;

/// Searches the system PATH for an executable.
pub fn find_exe_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_executable())
}

// utils.rs
pub fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

pub fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

pub fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(is_ident_start) && chars.all(is_ident_char)
}
