use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetMeterError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Test already running")]
    TestAlreadyRunning,

    #[error("No test currently running")]
    NoTestRunning,

    #[error("Namespace error: {0}")]
    Namespace(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}
