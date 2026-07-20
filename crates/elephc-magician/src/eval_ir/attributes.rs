//! Purpose:
//! Defines eval attribute declarations and literal argument metadata.
//!
//! Called from:
//! - Class-like/callable parsing, validation, context registration, and Reflection.
//!
//! Key details:
//! - Attribute arguments remain syntax values until explicitly materialized by the interpreter.

/// Literal attribute argument metadata retained by eval declarations.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalAttributeArg {
    String(String),
    Int(i64),
    Float(u64),
    Bool(bool),
    Null,
    Array(Vec<EvalAttributeArg>),
    Named {
        name: String,
        value: Box<EvalAttributeArg>,
    },
    IntKeyed {
        key: i64,
        value: Box<EvalAttributeArg>,
    },
}

impl EvalAttributeArg {
    /// Returns the PHP named-argument key when this attribute arg is named.
    pub fn name(&self) -> Option<&str> {
        match self {
            EvalAttributeArg::Named { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Returns the PHP integer array key when this attribute arg is int-keyed.
    pub fn int_key(&self) -> Option<i64> {
        match self {
            EvalAttributeArg::IntKeyed { key, .. } => Some(*key),
            _ => None,
        }
    }

    /// Returns the scalar payload, unwrapping a named or int-keyed wrapper.
    pub fn value(&self) -> &EvalAttributeArg {
        match self {
            EvalAttributeArg::Named { value, .. } | EvalAttributeArg::IntKeyed { value, .. } => {
                value
            }
            _ => self,
        }
    }
}

/// Attribute metadata retained for eval class-like declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalAttribute {
    name: String,
    args: Option<Vec<EvalAttributeArg>>,
}

impl EvalAttribute {
    /// Creates one eval attribute metadata entry.
    pub fn new(name: impl Into<String>, args: Option<Vec<EvalAttributeArg>>) -> Self {
        Self {
            name: name.into(),
            args,
        }
    }

    /// Returns the resolved PHP-visible attribute class name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns supported literal positional args, or `None` for unsupported metadata.
    pub fn args(&self) -> Option<&[EvalAttributeArg]> {
        self.args.as_deref()
    }
}
