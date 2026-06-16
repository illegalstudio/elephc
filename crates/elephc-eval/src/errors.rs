//! Purpose:
//! Defines stable integer status codes returned by the eval bridge ABI.
//! Keeps Rust error shapes internal while exposing C-compatible outcomes.
//!
//! Called from:
//! - `crate::__elephc_eval_execute()`
//!
//! Key details:
//! - Numeric values are part of the ABI contract and must remain stable.

/// Stable eval bridge status codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalStatus {
    Ok,
    ParseError,
    RuntimeFatal,
    UncaughtThrowable,
    UnsupportedConstruct,
    AbiMismatch,
}

impl EvalStatus {
    /// Returns the C ABI integer code for this status.
    pub const fn code(self) -> i32 {
        match self {
            Self::Ok => 0,
            Self::ParseError => 1,
            Self::RuntimeFatal => 2,
            Self::UncaughtThrowable => 3,
            Self::UnsupportedConstruct => 4,
            Self::AbiMismatch => 5,
        }
    }
}

/// Parse failures detected before lowering a runtime eval fragment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalParseError {
    PhpOpenTag,
    InvalidUtf8,
    UnsupportedConstruct,
    UnexpectedToken,
    UnexpectedEof,
    InvalidNumber,
    UnterminatedString,
    UnterminatedComment,
    ExpectedVariable,
    ExpectedSemicolon,
}

impl EvalParseError {
    /// Returns the ABI status that should be reported for this parse failure.
    pub const fn status(self) -> EvalStatus {
        match self {
            Self::UnsupportedConstruct => EvalStatus::UnsupportedConstruct,
            Self::PhpOpenTag
            | Self::InvalidUtf8
            | Self::UnexpectedToken
            | Self::UnexpectedEof
            | Self::InvalidNumber
            | Self::UnterminatedString
            | Self::UnterminatedComment
            | Self::ExpectedVariable
            | Self::ExpectedSemicolon => EvalStatus::ParseError,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies only known unsupported syntax maps to the unsupported ABI status.
    #[test]
    fn parse_error_status_distinguishes_unsupported_constructs() {
        assert_eq!(
            EvalParseError::UnsupportedConstruct.status(),
            EvalStatus::UnsupportedConstruct
        );
        assert_eq!(
            EvalParseError::UnexpectedToken.status(),
            EvalStatus::ParseError
        );
    }
}
