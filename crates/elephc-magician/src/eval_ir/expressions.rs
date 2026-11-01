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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum EvalExpr {
    /// Native default metadata raises an Error lazily, without crossing a compiled callback.
    DeferredDefaultError(String),
    Array(Vec<EvalArrayElement>),
    ArrayGet {
        array: Box<EvalExpr>,
        index: Box<EvalExpr>,
    },
    ArrayDestructureAssign {
        targets: Vec<Option<EvalDestructureTarget>>,
        value: Box<EvalExpr>,
    },
    Call {
        /// Resolved source spelling, normalized only when looking up the function.
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
        /// Resolved source spelling, retained for undefined-function diagnostics.
        name: String,
        /// Unqualified source spelling for PHP's global namespace fallback.
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
    /// A variable named at run time: `$$name` or `${expr}`. The expression yields the name.
    DynamicVar(Box<EvalExpr>),
    Match {
        subject: Box<EvalExpr>,
        arms: Vec<EvalMatchArm>,
        default: Option<Box<EvalExpr>>,
    },
    Clone(Box<EvalExpr>),
    NamespacedCall {
        /// Qualified source spelling, retained if namespace and global lookup both fail.
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
    NullCoalesceAssign {
        target: Box<EvalExpr>,
        default: Box<EvalExpr>,
    },
    CompoundAssign {
        target: Box<EvalExpr>,
        op: EvalBinOp,
        value: Box<EvalExpr>,
    },
    PostfixIncDec {
        target: Box<EvalExpr>,
        increment: bool,
    },
    Assign {
        target: Box<EvalExpr>,
        value: Box<EvalExpr>,
    },
    /// `$name = &<lvalue>` where the result is used, as in `if (null !== $e = &self::$c[$k])`.
    ///
    /// The statement form of the same binding is `EvalStmt::VarReferenceBind`; this one exists
    /// because PHP's `=` accepts a `&` source wherever an assignment is an expression, and
    /// `symfony/config/Resource/ClassExistenceResource.php` puts one inside an `if` condition.
    /// It evaluates to the value the name now aliases, which is what PHP's assignment yields.
    ReferenceBindAssign {
        target: String,
        source: Box<EvalExpr>,
    },
    /// `$target[]` naming the element an append WOULD create, valid only as a reference source.
    ///
    /// `$closure = &$this->optimized[$e][];` creates the element, leaves it null and binds to
    /// it. PHP refuses the same syntax where a value is wanted -- `echo $a[];` is the fatal
    /// `Cannot use [] for reading` -- so this node never reaches `eval_expr`.
    ArrayAppendSlot {
        target: Box<EvalExpr>,
    },
    /// `TARGET = &SOURCE` used where a VALUE is wanted, as in `if (null !== $e = &self::$c[$k])`.
    ///
    /// PHP's reference assignment is an expression. Its value is the bound value as a COPY, not
    /// a second alias: after `$a = ($b = &$one); $one = 9;` php reports `$b` as 9 and `$a` as
    /// the 1 it copied.
    ReferenceBind {
        target: Box<EvalExpr>,
        source: Box<EvalExpr>,
    },
    /// `$target[] = value` used where a VALUE is wanted, as in `return $this->rules[] = $r;`.
    ///
    /// PHP's append is an ordinary assignment expression whose value is the assigned one, so it
    /// nests (`$a[] = $b[] = 'x'`) and can stand anywhere an expression can. The statement
    /// spellings (`EvalStmt::ArrayAppendVar` and friends) stay as they are; this is what the
    /// expression parser builds when an append is not the whole statement.
    ArrayAppendAssign {
        target: Box<EvalExpr>,
        value: Box<EvalExpr>,
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
    Throw(Box<EvalExpr>),
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

impl EvalExpr {
    /// Whether this is a call whose RESULT can take an append: `app(Bag::class)[] = $v` appends
    /// through the returned object's `offsetSet()` (Laravel's `defer()` does this). php accepts
    /// it for any call; an array result is a temporary, so the append has no effect.
    pub(crate) fn is_call_result(&self) -> bool {
        matches!(
            self,
            EvalExpr::Call { .. }
                | EvalExpr::NamespacedCall { .. }
                | EvalExpr::DynamicCall { .. }
                | EvalExpr::MethodCall { .. }
                | EvalExpr::DynamicMethodCall { .. }
                | EvalExpr::NullsafeMethodCall { .. }
                | EvalExpr::NullsafeDynamicMethodCall { .. }
                | EvalExpr::StaticMethodCall { .. }
                | EvalExpr::DynamicStaticMethodCall { .. }
        )
    }
}

/// The right-hand side accepted by PHP's `instanceof` operator.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum EvalInstanceOfTarget {
    ClassName(String),
    Expr(Box<EvalExpr>),
}

/// One source-order function or method call argument parsed from eval code.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum EvalArrayElement {
    Value(EvalExpr),
    Reference(EvalExpr),
    KeyValue { key: EvalExpr, value: EvalExpr },
    KeyReference { key: EvalExpr, value: EvalExpr },
    /// `...$operand` inside an array literal: PHP's unpacking, not the variadic parameter form.
    ///
    /// The operand's INTEGER keys are renumbered from the literal's own running key while its
    /// STRING keys are carried through untouched, so this is neither a plain value nor a keyed
    /// one and cannot be desugared into either.
    Spread(EvalExpr),
}

/// One ordered arm in a PHP `match` expression parsed from an eval fragment.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct EvalMatchArm {
    pub patterns: Vec<EvalExpr>,
    pub value: EvalExpr,
}

/// One ordered case arm in a PHP switch parsed from an eval fragment.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct EvalSwitchCase {
    pub condition: Option<EvalExpr>,
    pub body: Vec<EvalStmt>,
}

/// Literal syntax supported by the initial EvalIR parser.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum EvalConst {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    /// A literal containing bytes that cannot be stored in a Rust UTF-8 string.
    Bytes(Vec<u8>),
}

/// PHP magic constants supported by runtime eval fragments.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
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

/// One slot of a PHP list-destructuring pattern.
///
/// A pattern is NOT an array literal, which is why it has its own type: it may carry holes
/// (`[, , , $x]`), its slots are LVALUES rather than values (`[$this->x, $h["k"], S::$p]`), it
/// may name keys (`["a" => $x]`), and it nests (`[[$a, $b], $c]`). Modelling it as a list of
/// optional variable NAMES accepted only the simplest quarter of that.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct EvalDestructureTarget {
    /// The key to read, for the `["a" => $x]` form; positional slots read their index.
    pub key: Option<EvalExpr>,
    /// Where the element goes.
    pub slot: EvalDestructureSlot,
}

/// The destination of one destructuring slot.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum EvalDestructureSlot {
    /// Any writable PHP lvalue: a variable, a property, an element, a static property.
    Lvalue(EvalExpr),
    /// A nested pattern, for `[[$a, $b], $c] = ...`.
    Nested(Vec<Option<EvalDestructureTarget>>),
}

/// Cast targets supported by runtime eval expressions.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalCastType {
    Int,
    Float,
    String,
    Bool,
    Array,
    Object,
}

/// Unary operations supported by the initial EvalIR parser.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalUnaryOp {
    Plus,
    Negate,
    LogicalNot,
    BitNot,
    ErrorSuppress,
}

/// One `declare(name=VALUE)` right-hand side, as written.
///
/// php takes an integer for `ticks` and `strict_types` and a string for `encoding`, and the two
/// are not interchangeable: `declare(ticks="1")` is a parse error.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalDeclareValue {
    Int(i64),
    Str(String),
}
