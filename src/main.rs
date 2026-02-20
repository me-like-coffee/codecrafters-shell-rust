#[allow(unused_imports)]
use std::io::{self, Write};

use is_executable::IsExecutable;
use std::env;

fn find_exe(name: &str) -> Option<String> {
    if let Some(paths) = env::var_os("PATH") {
        for path in env::split_paths(&paths) {
            let p = path.join(name);
            if p.is_executable() {
                return Some(p.to_string_lossy().into_owned());
            }
        }
    }
    None
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
                    "exit" | "echo" | "type" => println!("{} is a shell builtin", cmd),
                    _ => {
                        if let Some(exe_path) = find_exe(cmd) {
                            println!("{} is {}", cmd, exe_path);
                        } else {
                            println!("{}: not found", cmd);
                        }
                    }
                },
                None => println!("type: missing argument"),
            },
            _ => {
                // Prints the "<command>: command not found" message
                println!("{}: command not found", command)
            }
        }
    }
}
