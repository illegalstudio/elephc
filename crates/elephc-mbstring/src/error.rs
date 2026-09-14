//! Purpose:
//! Defines PHP-visible mbstring operation failures shared by AOT and eval adapters.
//!
//! Called from:
//! - Shared mbstring operations before adapters materialize exceptions or diagnostics.
//!
//! Key details:
//! - Ordinary false returns are values, not exceptions.
//! - Binary argument diagnostics retain their original bytes without UTF-8 replacement.

/// A catchable PHP operation failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MbError {
    /// PHP `ValueError`, carrying its complete diagnostic text.
    Value(String),
    /// PHP `ValueError` whose original diagnostic contains non-UTF-8 argument bytes.
    ValueBytes(Vec<u8>),
    /// PHP `Error`, for example an overflowing requested string allocation.
    Runtime(String),
}

/// Result of an operation that can raise a PHP exception.
pub type MbResult<T> = Result<T, MbError>;

impl MbError {
    /// Retains a binary argument value in PHP's quoted argument diagnostic.
    pub fn argument_value(function: &str, number: usize, parameter: &str, prefix: &str, value: &[u8], suffix: &str) -> Self {
        let mut message = format!("{function}(): Argument #{number} (${parameter}) {prefix}\"").into_bytes();
        message.extend_from_slice(value.split(|&byte| byte == 0).next().unwrap_or_default());
        message.extend_from_slice(format!("\"{suffix}").as_bytes());
        match String::from_utf8(message) {
            Ok(message) => Self::Value(message),
            Err(error) => Self::ValueBytes(error.into_bytes()),
        }
    }

    /// Formats PHP's argument-specific ValueError diagnostic.
    pub fn argument(function: &str, number: usize, parameter: &str, detail: &str) -> Self {
        Self::Value(format!("{function}(): Argument #{number} (${parameter}) {detail}"))
    }

    /// Formats PHP's nonempty-argument requirement.
    pub fn empty(function: &str, number: usize, parameter: &str) -> Self {
        Self::argument(function, number, parameter, "must not be empty")
    }
}
