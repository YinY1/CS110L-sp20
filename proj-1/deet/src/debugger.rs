use std::collections::HashMap;

use crate::debugger_command::DebuggerCommand;
use crate::dwarf_data::{DwarfData, Error as DwarfError};
use crate::inferior::{Inferior, Status};
use rustyline::error::ReadlineError;
use rustyline::Editor;

pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<()>,
    inferior: Option<Inferior>,
    debug_data: DwarfData,
    break_points: HashMap<usize, u8>,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        let debug_data = match DwarfData::from_file(target) {
            Ok(val) => val,
            Err(DwarfError::ErrorOpeningFile) => {
                println!("Could not open file {}", target);
                std::process::exit(1);
            }
            Err(DwarfError::DwarfFormatError(err)) => {
                println!("Could not debugging symbols from {}: {:?}", target, err);
                std::process::exit(1);
            }
        };

        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<()>::new();
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);

        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
            debug_data,
            break_points: HashMap::new(),
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    if self.inferior.is_some() {
                        self.get_inferior_as_mut()
                            .kill()
                            .expect("Error killing inferior");
                    }
                    if let Some(inferior) =
                        Inferior::new(&self.target, &args, &mut self.break_points)
                    {
                        // Create the inferior
                        self.inferior = Some(inferior);
                        // You may use self.inferior.as_mut().unwrap() to get a mutable reference
                        // to the Inferior object
                        self.run_inferior();
                    } else {
                        println!("Error starting subprocess");
                    }
                }
                DebuggerCommand::Quit => {
                    if self.inferior.is_some() {
                        self.get_inferior_as_mut()
                            .kill()
                            .expect("Error killing inferior");
                    }
                    return;
                }
                DebuggerCommand::Continue => {
                    if self.inferior.is_none() {
                        println!("No inferior is running");
                    } else {
                        self.run_inferior();
                    }
                }
                DebuggerCommand::Backtrace => {
                    self.get_inferior_as_ref()
                        .print_backtrace(&self.debug_data)
                        .expect("Error backtracing");
                }
                DebuggerCommand::Break(target) => {
                    let addr = if let Some(address) = target.strip_prefix('*') {
                        if let Some(avalible) = parse_address(address) {
                            avalible
                        } else {
                            println!("Error address");
                            continue;
                        }
                    } else if let Ok(line_number) = target.parse::<usize>() {
                        if let Some(address) = self.debug_data.get_addr_for_line(None, line_number)
                        {
                            address
                        } else {
                            println!("Incorrect line number");
                            continue;
                        }
                    } else if let Some(address) =
                        self.debug_data.get_addr_for_function(None, &target)
                    {
                        address
                    } else {
                        println!("Function name not found");
                        continue;
                    };

                    println!("Set break point {} at {:#x}", self.break_points.len(), addr);
                    self.break_points.insert(addr, 0);
                }
            }
        }
    }

    fn run_inferior(&mut self) {
        let status = self
            .inferior
            .as_mut()
            .unwrap()
            .wake_up(&self.break_points)
            .expect("Error getting inferior status");

        match status {
            Status::Stopped(signal, rip) => {
                println!("Child stopped (signal {signal})");
                let line = self.debug_data.get_line_from_addr(rip).unwrap();
                println!("Stopped at {}", line);
            }
            Status::Exited(exit_code) => {
                println!("Child exited (status: {exit_code})");
            }
            Status::Signaled(signal) => {
                println!("Child exited (signal {signal})");
            }
        }
    }

    fn get_inferior_as_mut(&mut self) -> &mut Inferior {
        self.inferior.as_mut().unwrap()
    }

    fn get_inferior_as_ref(&self) -> &Inferior {
        self.inferior.as_ref().unwrap()
    }

    /// This function prompts the user to enter a command, and continues re-prompting until the user
    /// enters a valid command. It uses DebuggerCommand::from_tokens to do the command parsing.
    ///
    /// You don't need to read, understand, or modify this function.
    fn get_next_command(&mut self) -> DebuggerCommand {
        loop {
            // Print prompt and get next line of user input
            match self.readline.readline("(deet) ") {
                Err(ReadlineError::Interrupted) => {
                    // User pressed ctrl+c. We're going to ignore it
                    println!("Type \"quit\" to exit");
                }
                Err(ReadlineError::Eof) => {
                    // User pressed ctrl+d, which is the equivalent of "quit" for our purposes
                    return DebuggerCommand::Quit;
                }
                Err(err) => {
                    panic!("Unexpected I/O error: {:?}", err);
                }
                Ok(line) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    self.readline.add_history_entry(line.as_str());
                    if let Err(err) = self.readline.save_history(&self.history_path) {
                        println!(
                            "Warning: failed to save history file at {}: {}",
                            self.history_path, err
                        );
                    }
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if let Some(cmd) = DebuggerCommand::from_tokens(&tokens) {
                        return cmd;
                    } else {
                        println!("Unrecognized command.");
                    }
                }
            }
        }
    }
}

fn parse_address(addr: &str) -> Option<usize> {
    let addr_without_0x = if addr.to_lowercase().starts_with("0x") {
        &addr[2..]
    } else {
        addr
    };
    usize::from_str_radix(addr_without_0x, 16).ok()
}
