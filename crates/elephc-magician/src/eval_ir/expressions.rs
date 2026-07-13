//! Purpose:
//! Defines EvalIR expressions, calls, arrays, constants, operators, and match/switch values.
//!
//! Called from:
//! - Expression parser, statement nodes, optimizer-free eval execution, and default metadata.
//!
//! Key details:
//! - Expression nodes describe syntax and evaluation order without owning runtime cells.

use super::*;

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
        args: Vec<EvalCallArg>,
    },
    Cast {
        target: EvalCastType,
        expr: Box<EvalExpr>,
    },
    Const(EvalConst),
    ConstFetch(String),
    Closure {
        function: EvalFunction,
        captures: Vec<EvalClosureCapture>,
        is_static: bool,
    },
    FunctionCallable {
        name: String,
        fallback_name: Option<String>,
    },
    InvokableCallable {
        object: Box<EvalExpr>,
    },
    MethodCallable {
        object: Box<EvalExpr>,
        method: Box<EvalExpr>,
    },
    StaticMethodCallable {
        class_name: String,
        method: Box<EvalExpr>,
    },
    DynamicStaticMethodCallable {
        class_name: Box<EvalExpr>,
        method: Box<EvalExpr>,
    },
    DynamicCall {
        callee: Box<EvalExpr>,
        args: Vec<EvalCallArg>,
    },
    DynamicMethodCall {
        object: Box<EvalExpr>,
        method: Box<EvalExpr>,
        args: Vec<EvalCallArg>,
    },
    DynamicNewObject {
        class_name: Box<EvalExpr>,
        args: Vec<EvalCallArg>,
    },
    DynamicPropertyGet {
        object: Box<EvalExpr>,
        property: Box<EvalExpr>,
    },
    DynamicStaticMethodCall {
        class_name: Box<EvalExpr>,
        method: Box<EvalExpr>,
        args: Vec<EvalCallArg>,
    },
    DynamicStaticPropertyGet {
        class_name: Box<EvalExpr>,
        property: String,
    },
    DynamicStaticPropertyNameGet {
        class_name: Box<EvalExpr>,
        property: Box<EvalExpr>,
    },
    DynamicClassConstantFetch {
        class_name: Box<EvalExpr>,
        constant: String,
    },
    DynamicClassConstantNameFetch {
        class_name: Box<EvalExpr>,
        constant: Box<EvalExpr>,
    },
    DynamicClassNameFetch {
        class_name: Box<EvalExpr>,
    },
    Include {
        path: Box<EvalExpr>,
        required: bool,
        once: bool,
    },
    InstanceOf {
        value: Box<EvalExpr>,
        target: EvalInstanceOfTarget,
    },
    LoadVar(String),
    Match {
        subject: Box<EvalExpr>,
        arms: Vec<EvalMatchArm>,
        default: Option<Box<EvalExpr>>,
    },
    Clone(Box<EvalExpr>),
    NamespacedCall {
        name: String,
        fallback_name: String,
        args: Vec<EvalCallArg>,
    },
    NamespacedConstFetch {
        name: String,
        fallback_name: String,
    },
    MethodCall {
        object: Box<EvalExpr>,
        method: String,
        args: Vec<EvalCallArg>,
    },
    NullsafeMethodCall {
        object: Box<EvalExpr>,
        method: String,
        args: Vec<EvalCallArg>,
    },
    NullsafeDynamicMethodCall {
        object: Box<EvalExpr>,
        method: Box<EvalExpr>,
        args: Vec<EvalCallArg>,
    },
    Magic(EvalMagicConst),
    NewObject {
        class_name: String,
        args: Vec<EvalCallArg>,
    },
    NewAnonymousClass {
        class: EvalClass,
        args: Vec<EvalCallArg>,
    },
    StaticMethodCall {
        class_name: String,
        method: String,
        args: Vec<EvalCallArg>,
    },
    StaticPropertyGet {
        class_name: String,
        property: String,
    },
    ClassConstantFetch {
        class_name: String,
        constant: String,
    },
    ClassNameFetch {
        class_name: String,
    },
    NullCoalesce {
        value: Box<EvalExpr>,
        default: Box<EvalExpr>,
    },
    NullsafePropertyGet {
        object: Box<EvalExpr>,
        property: String,
    },
    NullsafeDynamicPropertyGet {
        object: Box<EvalExpr>,
        property: Box<EvalExpr>,
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

/// The right-hand side accepted by PHP's `instanceof` operator.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalInstanceOfTarget {
    ClassName(String),
    Expr(Box<EvalExpr>),
}

/// One source-order function or method call argument parsed from eval code.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalCallArg {
    name: Option<String>,
    spread: bool,
    value: EvalExpr,
}

impl EvalCallArg {
    /// Creates a positional call argument from a value expression.
    pub fn positional(value: EvalExpr) -> Self {
        Self {
            name: None,
            spread: false,
            value,
        }
    }

    /// Creates a named call argument from a parameter name and value expression.
    pub fn named(name: impl Into<String>, value: EvalExpr) -> Self {
        Self {
            name: Some(name.into()),
            spread: false,
            value,
        }
    }

    /// Creates an unpacking call argument from an array expression.
    pub fn spread(value: EvalExpr) -> Self {
        Self {
            name: None,
            spread: true,
            value,
        }
    }

    /// Returns the source argument name without `$`, if the argument was named.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns true when this argument came from `...expr` unpacking syntax.
    pub const fn is_spread(&self) -> bool {
        self.spread
    }

    /// Returns the expression that computes this argument's runtime value.
    pub const fn value(&self) -> &EvalExpr {
        &self.value
    }
}

/// One element in a PHP array literal parsed from an eval fragment.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalArrayElement {
    Value(EvalExpr),
    Reference(EvalExpr),
    KeyValue { key: EvalExpr, value: EvalExpr },
    KeyReference { key: EvalExpr, value: EvalExpr },
}

/// One ordered arm in a PHP `match` expression parsed from an eval fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalMatchArm {
    pub patterns: Vec<EvalExpr>,
    pub value: EvalExpr,
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
    Pow,
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
    Spaceship,
}

/// Scalar cast targets supported by runtime eval expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalCastType {
    Int,
    Float,
    String,
    Bool,
}

/// Unary operations supported by the initial EvalIR parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalUnaryOp {
    Plus,
    Negate,
    LogicalNot,
    BitNot,
}
