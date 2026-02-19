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

        // Prints the "<command>: command not found" message
        println!("{}: command not found", command.trim())
    }
}
