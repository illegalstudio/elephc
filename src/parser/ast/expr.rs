//! Purpose:
//! Defines expression AST nodes for PHP expressions and elephc expression-level extensions.
//! Carries operands, call forms, access forms, magic constants, and source spans.
//!
//! Called from:
//! - `crate::parser::expr` and AST-walking passes such as resolver, name resolver, optimizer, and codegen.
//!
//! Key details:
//! - Expression variants must preserve PHP evaluation shape so later passes can model side effects correctly.

use crate::names::Name;
use crate::span::Span;

use super::{BinOp, Stmt, TypeExpr};

// --- Expressions ---

#[derive(Debug, Clone)]
/// Expression AST node.
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
/// Expression kind.
pub enum ExprKind {
    StringLiteral(String),
    IntLiteral(i64),
    FloatLiteral(f64),
    Variable(String),
    BinaryOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    InstanceOf {
        value: Box<Expr>,
        target: InstanceOfTarget,
    },
    BoolLiteral(bool),
    Null,
    Negate(Box<Expr>),
    Not(Box<Expr>),
    BitNot(Box<Expr>),
    Throw(Box<Expr>),
    ErrorSuppress(Box<Expr>),
    Print(Box<Expr>),
    NullCoalesce {
        value: Box<Expr>,
        default: Box<Expr>,
    },
    /// PHP 8.5 pipe operator: `value |> callable` evaluates `value`, then invokes
    /// `callable` with the resulting value as the single positional argument.
    /// Left-associative; LHS is observably evaluated before RHS.
    Pipe {
        value: Box<Expr>,
        callable: Box<Expr>,
    },
    Assignment {
        target: Box<Expr>,
        value: Box<Expr>,
        result_target: Option<Box<Expr>>,
        prelude: Vec<Stmt>,
        conditional_value_temp: Option<String>,
    },
    PreIncrement(String),
    PostIncrement(String),
    PreDecrement(String),
    PostDecrement(String),
    FunctionCall {
        name: Name,
        args: Vec<Expr>,
    },
    ArrayLiteral(Vec<Expr>),
    ArrayLiteralAssoc(Vec<(Expr, Expr)>),
    Match {
        subject: Box<Expr>,
        arms: Vec<(Vec<Expr>, Expr)>,
        default: Option<Box<Expr>>,
    },
    ArrayAccess {
        array: Box<Expr>,
        index: Box<Expr>,
    },
    Ternary {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    ShortTernary {
        value: Box<Expr>,
        default: Box<Expr>,
    },
    Cast {
        target: CastType,
        expr: Box<Expr>,
    },
    Closure {
        params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
        variadic: Option<String>,
        return_type: Option<TypeExpr>,
        body: Vec<Stmt>,
        is_arrow: bool,
        is_static: bool,
        captures: Vec<String>,
        capture_refs: Vec<String>,
    },
    NamedArg {
        name: String,
        value: Box<Expr>,
    },
    Spread(Box<Expr>),
    ClosureCall {
        var: String,
        args: Vec<Expr>,
    },
    ExprCall {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    ConstRef(Name),
    NewObject {
        class_name: Name,
        args: Vec<Expr>,
    },
    /// PHP `new $var()` / `new $var(args)` — the class is named at runtime
    /// by a string expression. Resolved through the runtime class table at
    /// codegen time (`__rt_new_by_name`).
    NewDynamic {
        name_expr: Box<Expr>,
        args: Vec<Expr>,
    },
    /// Internal synthetic factory used by compiler-provided methods that must
    /// construct an object from a runtime class-string while constraining it to
    /// a known parent class.
    NewDynamicObject {
        class_name: Box<Expr>,
        fallback_class: Name,
        required_parent: Name,
        args: Vec<Expr>,
    },
    PropertyAccess {
        object: Box<Expr>,
        property: String,
    },
    DynamicPropertyAccess {
        object: Box<Expr>,
        property: Box<Expr>,
    },
    NullsafePropertyAccess {
        object: Box<Expr>,
        property: String,
    },
    NullsafeDynamicPropertyAccess {
        object: Box<Expr>,
        property: Box<Expr>,
    },
    StaticPropertyAccess {
        receiver: StaticReceiver,
        property: String,
    },
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    NullsafeMethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    StaticMethodCall {
        receiver: StaticReceiver,
        method: String,
        args: Vec<Expr>,
    },
    FirstClassCallable(CallableTarget),
    This,
    PtrCast {
        target_type: String,
        expr: Box<Expr>,
    },
    BufferNew {
        element_type: TypeExpr,
        len: Box<Expr>,
    },
    /// `MyClass::class`, `self::class`, `parent::class`, `static::class`.
    /// For `Named`, `Self_`, `Parent` the FQN is resolved at compile time;
    /// `Static` resolves the called class via late static binding.
    ClassConstant {
        receiver: StaticReceiver,
    },
    /// Access to a user-declared class constant: `MyClass::FOO`,
    /// `self::FOO`, `parent::FOO`, `static::FOO`. Resolved at type-check
    /// time by looking up the constant in the receiver's class info.
    ScopedConstantAccess {
        receiver: StaticReceiver,
        name: String,
    },
    /// `new self()`, `new static()`, `new parent()`. Distinct from `NewObject`
    /// which uses a fixed class name; this variant carries a `StaticReceiver`
    /// so that codegen can apply late static binding for `static`.
    NewScopedObject {
        receiver: StaticReceiver,
        args: Vec<Expr>,
    },
    MagicConstant(MagicConstant),
    Yield {
        key: Option<Box<Expr>>,
        value: Option<Box<Expr>>,
    },
    YieldFrom(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
/// Magic constant.
pub enum MagicConstant {
    Dir,
    File,
    Function,
    Class,
    Method,
    Namespace,
    Trait,
}

#[derive(Debug, Clone, PartialEq)]
/// Cast type.
pub enum CastType {
    Int,
    Float,
    String,
    Bool,
    Array,
}

#[derive(Debug, Clone, PartialEq)]
/// Static receiver.
pub enum StaticReceiver {
    Named(Name),
    Self_,
    Static,
    Parent,
}

#[derive(Debug, Clone, PartialEq)]
/// InstanceOf target.
pub enum InstanceOfTarget {
    Name(Name),
    Expr(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
/// Callable target.
pub enum CallableTarget {
    Function(Name),
    StaticMethod {
        receiver: StaticReceiver,
        method: String,
    },
    Method {
        object: Box<Expr>,
        method: String,
    },
}

impl PartialEq for Expr {
    /// Compares two expressions by comparing their `ExprKind`s only.
    /// Spans are not considered in equality.
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

#[allow(dead_code)] // Constructors used by test crate
impl Expr {
    /// Constructs an expression with the given kind and span.
    /// Typically used by parsers; test code may also use this directly.
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Constructs a string literal expression with the given value.
    pub fn string_lit(s: impl Into<String>) -> Self {
        Self::new(ExprKind::StringLiteral(s.into()), Span::dummy())
    }

    /// Constructs an integer literal expression with the given value.
    pub fn int_lit(n: i64) -> Self {
        Self::new(ExprKind::IntLiteral(n), Span::dummy())
    }

    /// Constructs a float literal expression with the given value.
    pub fn float_lit(f: f64) -> Self {
        Self::new(ExprKind::FloatLiteral(f), Span::dummy())
    }

    /// Constructs a variable expression with the given name.
    pub fn var(name: impl Into<String>) -> Self {
        Self::new(ExprKind::Variable(name.into()), Span::dummy())
    }

    /// Constructs a binary operation expression with the given left operand, operator, and right operand.
    /// Both operands are boxed and evaluated left-to-right (PHP evaluation order).
    pub fn binop(left: Expr, op: BinOp, right: Expr) -> Self {
        Self::new(
            ExprKind::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            },
            Span::dummy(),
        )
    }

    /// Constructs an `instanceof` expression with a static class name target.
    /// The target is resolved at type-check time.
    pub fn instance_of(value: Expr, target: Name) -> Self {
        Self::new(
            ExprKind::InstanceOf {
                value: Box::new(value),
                target: InstanceOfTarget::Name(target),
            },
            Span::dummy(),
        )
    }

    /// Constructs a dynamic `instanceof` expression where the target is an expression
    /// evaluated at runtime rather than a static class name.
    pub fn dynamic_instance_of(value: Expr, target: Expr) -> Self {
        Self::new(
            ExprKind::InstanceOf {
                value: Box::new(value),
                target: InstanceOfTarget::Expr(Box::new(target)),
            },
            Span::dummy(),
        )
    }

    /// Constructs a negation expression for the given expression.
    pub fn negate(inner: Expr) -> Self {
        Self::new(ExprKind::Negate(Box::new(inner)), Span::dummy())
    }

    /// Constructs a `print` expression that outputs the given expression's value.
    pub fn print(inner: Expr) -> Self {
        Self::new(ExprKind::Print(Box::new(inner)), Span::dummy())
    }
}
