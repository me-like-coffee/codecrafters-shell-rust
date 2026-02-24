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
