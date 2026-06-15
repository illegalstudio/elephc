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
    UnexpectedToken,
    UnexpectedEof,
    InvalidNumber,
    UnterminatedString,
    ExpectedVariable,
    ExpectedSemicolon,
}
