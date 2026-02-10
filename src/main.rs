mod cli;
mod monitor;
mod check;
mod utils;

use clap::Parser;
use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();
    
    let result = match cli.command {
        Commands::Monitor { directory, show_container, format } => {
            monitor::run_monitor(&directory, show_container, &format)
        }
        Commands::Check { container, output, verbose } => {
            check::run_check(container, &output, verbose)
        }
    };
    
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}