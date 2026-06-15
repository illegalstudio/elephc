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
    DoWhile {
        body: Vec<EvalStmt>,
        condition: EvalExpr,
    },
    Echo(EvalExpr),
    For {
        init: Vec<EvalStmt>,
        condition: Option<EvalExpr>,
        update: Vec<EvalStmt>,
        body: Vec<EvalStmt>,
    },
    Foreach {
        array: EvalExpr,
        key_name: Option<String>,
        value_name: String,
        body: Vec<EvalStmt>,
    },
    FunctionDecl {
        name: String,
        params: Vec<String>,
        body: Vec<EvalStmt>,
    },
    If {
        condition: EvalExpr,
        then_branch: Vec<EvalStmt>,
        else_branch: Vec<EvalStmt>,
    },
    Return(Option<EvalExpr>),
    PropertySet {
        object: EvalExpr,
        property: String,
        value: EvalExpr,
    },
    StoreVar {
        name: String,
        value: EvalExpr,
    },
    Switch {
        expr: EvalExpr,
        cases: Vec<EvalSwitchCase>,
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

/// Runtime user function declared by an eval fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalFunction {
    name: String,
    params: Vec<String>,
    body: Vec<EvalStmt>,
}

impl EvalFunction {
    /// Creates a dynamic eval function with source-order parameters and body.
    pub fn new(name: impl Into<String>, params: Vec<String>, body: Vec<EvalStmt>) -> Self {
        Self {
            name: name.into(),
            params,
            body,
        }
    }

    /// Returns the original source spelling of this eval-declared function name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns source-order parameter names without leading `$`.
    pub fn params(&self) -> &[String] {
        &self.params
    }

    /// Returns the dynamic EvalIR statements that form the function body.
    pub fn body(&self) -> &[EvalStmt] {
        &self.body
    }
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
    MethodCall {
        object: Box<EvalExpr>,
        method: String,
        args: Vec<EvalExpr>,
    },
    Magic(EvalMagicConst),
    NullCoalesce {
        value: Box<EvalExpr>,
        default: Box<EvalExpr>,
    },
    PropertyGet {
        object: Box<EvalExpr>,
        property: String,
    },
    Print(Box<EvalExpr>),
    Ternary {
        condition: Box<EvalExpr>,
        then_branch: Option<Box<EvalExpr>>,
        else_branch: Box<EvalExpr>,
    },
    Unary {
        op: EvalUnaryOp,
        expr: Box<EvalExpr>,
    },
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

/// One ordered case arm in a PHP switch parsed from an eval fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalSwitchCase {
    pub condition: Option<EvalExpr>,
    pub body: Vec<EvalStmt>,
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

/// PHP magic constants supported by runtime eval fragments.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalMagicConst {
    File,
    Dir,
    Line(i64),
    Function,
    Class,
    Method,
    Namespace,
    Trait,
}

/// Binary operations supported by the initial EvalIR parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    Concat,
    LogicalAnd,
    LogicalOr,
    LogicalXor,
    LooseEq,
    LooseNotEq,
    StrictEq,
    StrictNotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

/// Unary operations supported by the initial EvalIR parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalUnaryOp {
    Plus,
    Negate,
    LogicalNot,
    BitNot,
}
