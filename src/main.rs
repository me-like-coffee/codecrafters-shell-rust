// #[allow(unused_imports)]
use std::io::{self, Write};

use is_executable::IsExecutable;
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn find_exe_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_executable())
}

fn main() {
    loop {
        // TODO: Uncomment the code below to pass the first stage
        print!("$ ");
        io::stdout().flush().unwrap();

        // Captures the user's command in the "command" variable
        let mut command = String::new();
        io::stdin().read_line(&mut command).unwrap();

        let mut command_and_args = command.split_whitespace();
        let command = command_and_args.next().unwrap_or("");
        let args: Vec<&str> = command_and_args.collect();

        match command {
            "exit" => break, // Exits the shell if the user types "exit"
            "echo" => {
                // Prints the arguments passed to the "echo" command
                println!("{}", args.join(" "));
            }
            // Prints if the command is a shell buitlin, executable, or not found
            "type" => match args.first() {
                Some(&cmd) => match cmd {
                    "exit" | "echo" | "type" | "pwd" | "cd" => {
                        println!("{} is a shell builtin", cmd)
                    }
                    _ => {
                        if let Some(exe_path) = find_exe_path(cmd) {
                            println!("{} is {}", cmd, exe_path.to_str().unwrap());
                        } else {
                            println!("{}: not found", cmd);
                        }
                    }
                },
                None => println!("type: missing argument"),
            },
            "pwd" => {
                // Prints the current working directory
                match env::current_dir() {
                    Ok(path) => println!("{}", path.display()),
                    Err(e) => println!("Error getting current directory: {}", e),
                }
            }
            "cd" => {
                // Changes the current working directory to the specified path
                if let Some(path_str) = args.first() {
                    // Handle '~' by replacing it with the home directory
                    let target_path = if path_str == &"~" {
                        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
                    } else {
                        std::path::PathBuf::from(path_str)
                    };
                    if env::set_current_dir(&target_path).is_err() {
                        println!("cd: {}: No such file or directory", path_str);
                    }
                } else {
                    println!("cd: missing argument");
                }
            }
            _ => {
                // Tries to find the executable path of the command
                if find_exe_path(command).is_some() {
                    // Executes the command with the provided arguments
                    let mut child = Command::new(command)
                        .args(&args)
                        .spawn()
                        .expect("Failed to execute command");
                    child.wait().expect("Failed to wait on child");
                } else {
                    // Prints the "<command>: command not found" message
                    println!("{}: command not found", command);
                }
            }
        }
    }
}
