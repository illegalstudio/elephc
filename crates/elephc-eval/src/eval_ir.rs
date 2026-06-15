//! Purpose:
//! Defines the dynamic EvalIR used by runtime `eval()` fragments.
//! EvalIR models by-name variable operations and expression shape without
//! introducing an independent runtime value representation.
//!
//! Called from:
//! - `crate::parser::parse_fragment()`
//! - Future `crate::interpreter` execution.
//!
//! Key details:
//! - Runtime execution must turn constants into elephc runtime cells through
//!   value-bridge hooks; EvalIR constants are syntax data, not owned PHP values.

/// Parsed eval fragment lowered into dynamic by-name statements.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalProgram {
    source_len: usize,
    statements: Vec<EvalStmt>,
}

impl EvalProgram {
    /// Creates an EvalIR program for a source fragment and statement list.
    pub fn new(source_len: usize, statements: Vec<EvalStmt>) -> Self {
        Self {
            source_len,
            statements,
        }
    }

    /// Returns the byte length of the parsed eval fragment.
    pub const fn source_len(&self) -> usize {
        self.source_len
    }

    /// Returns the ordered EvalIR statements in source order.
    pub fn statements(&self) -> &[EvalStmt] {
        &self.statements
    }

    /// Consumes the program and returns its statement list.
    pub fn into_statements(self) -> Vec<EvalStmt> {
        self.statements
    }
}

/// Dynamic eval statements that operate on a materialized activation scope.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalStmt {
    ArraySetVar {
        name: String,
        index: EvalExpr,
        value: EvalExpr,
    },
    Break,
    Continue,
    Echo(EvalExpr),
    For {
        init: Vec<EvalStmt>,
        condition: Option<EvalExpr>,
        update: Vec<EvalStmt>,
        body: Vec<EvalStmt>,
    },
    If {
        condition: EvalExpr,
        then_branch: Vec<EvalStmt>,
        else_branch: Vec<EvalStmt>,
    },
    Return(Option<EvalExpr>),
    StoreVar {
        name: String,
        value: EvalExpr,
    },
    UnsetVar {
        name: String,
    },
    While {
        condition: EvalExpr,
        body: Vec<EvalStmt>,
    },
    Expr(EvalExpr),
}

/// Dynamic eval expressions evaluated by the interpreter against runtime cells.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalExpr {
    Array(Vec<EvalArrayElement>),
    ArrayGet {
        array: Box<EvalExpr>,
        index: Box<EvalExpr>,
    },
    Call {
        name: String,
        args: Vec<EvalExpr>,
    },
    Const(EvalConst),
    LoadVar(String),
    Print(Box<EvalExpr>),
    Binary {
        op: EvalBinOp,
        left: Box<EvalExpr>,
        right: Box<EvalExpr>,
    },
}

/// One element in a PHP array literal parsed from an eval fragment.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalArrayElement {
    Value(EvalExpr),
    KeyValue { key: EvalExpr, value: EvalExpr },
}

/// Literal syntax supported by the initial EvalIR parser.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalConst {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

/// Binary operations supported by the initial EvalIR parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalBinOp {
    Add,
    Sub,
    Mul,
    Concat,
    LooseEq,
    LooseNotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}
