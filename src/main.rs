#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    loop {
        // TODO: Uncomment the code below to pass the first stage
        print!("$ ");
        io::stdout().flush().unwrap();

        // Captures the user's command in the "command" variable
        let mut command = String::new();
        io::stdin().read_line(&mut command).unwrap();

        let mut command_and_args = command.trim().split_whitespace();
        let command = command_and_args.next().unwrap_or("");
        let args: Vec<&str> = command_and_args.collect();

        match command {
            "exit" => break, // Exits the shell if the user types "exit"
            "echo" => {
                // Prints the arguments passed to the "echo" command
                println!("{}", args.join(" "));
            }
            _ => {
                        // Prints the "<command>: command not found" message
                    println!("{}: command not found", command.trim())

            }
        }

    }
}
