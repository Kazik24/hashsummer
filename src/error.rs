use thiserror::Error;

pub type SumResult<T> = Result<T, SumError>;

#[derive(Debug, Error)]
pub enum SumError {
    #[error("IO - {0}")]
    Io(#[from] std::io::Error),
}
