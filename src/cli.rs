use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sedock")]
#[command(version = concat!(env!("CARGO_PKG_VERSION"), " (built ", env!("BUILD_TIME"), ")"))]
#[command(about = "Docker monitoring and inspection tool", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Monitor file access in a directory
    #[command(arg_required_else_help = true)]
    Monitor {
        /// Directory to monitor
        #[arg(short, long)]
        directory: String,
        
        /// Output format (text or json)
        #[arg(short, long, default_value = "text")]
        format: String,
        
        /// Disable event deduplication (show all events)
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// Check and collect Docker container information
    Check {
        /// Specific container ID or name
        #[arg(short, long)]
        container: Option<String>,
        
        /// Output format (text or json)
        #[arg(short, long, default_value = "text")]
        output: String,
        
        /// Show detailed information
        #[arg(short, long, default_value = "false")]
        verbose: bool,
    },
}