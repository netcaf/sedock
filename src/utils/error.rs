use thiserror::Error;

#[derive(Error, Debug)]
pub enum SedockerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Permission denied: {0}")]
    Permission(String),
    
    #[error("Fanotify error: {0}")]
    Fanotify(String),
    
    #[error("Docker error: {0}")]
    Docker(String),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("System error: {0}")]
    System(String),
}

pub type Result<T> = std::result::Result<T, SedockerError>;