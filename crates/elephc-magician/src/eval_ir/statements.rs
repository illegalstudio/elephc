//! Purpose:
//! Defines EvalIR statement and catch-clause variants.
//!
//! Called from:
//! - Statement parser and interpreter statement dispatcher.
//!
//! Key details:
//! - Statements encode explicit mutation and structured control flow without runtime ownership.

use super::*;
use std::sync::Arc;

/// Dynamic eval statements that operate on a materialized activation scope.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum EvalStmt {
    GenericDecl(Box<EvalGenericTemplate>),
    TypedVar { name: String, declared: EvalParameterType, value: EvalExpr },
    ArrayAppendVar {
        name: String,
        value: EvalExpr,
    },
    ArrayAppend {
        target: EvalExpr,
        value: EvalExpr,
    },
    ArraySetVar {
        name: String,
        index: EvalExpr,
        value: EvalExpr,
    },
    ArrayDestructure {
        targets: Vec<Option<EvalDestructureTarget>>,
        value: EvalExpr,
    },
    /// `TARGET[] = &SOURCE`: binds a NEWLY APPENDED element of `target` to `source`.
    ///
    /// The target is a general lvalue rather than a bare name because PHP allows the append
    /// anywhere an array element can be written -- `$loops[$k][] = &$path;` appends through a
    /// nested element, which is `PhpDumper`'s shape.
    ArrayAppendReferenceBind {
        target: EvalExpr,
        source: EvalExpr,
    },
    ArrayReferenceBind {
        target: EvalExpr,
        source: EvalExpr,
    },
    Break(u32),
    Continue(u32),
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
    /// Shared, not owned: declaring the class hands this same allocation to the context, so a
    /// request that re-runs the file pays a reference-count bump instead of a deep copy of every
    /// method body. On a Symfony request that copy, and freeing it again at the request boundary,
    /// was 13% of the interpreter's work.
    ClassDecl(Arc<EvalClass>),
    EnumDecl(EvalEnum),
    InterfaceDecl(EvalInterface),
    Label(String),
    TraitDecl(EvalTrait),
    Foreach {
        array: EvalExpr,
        key_name: Option<String>,
        value_name: String,
        value_by_ref: bool,
        body: Vec<EvalStmt>,
    },
    FunctionDecl {
        name: String,
        source_location: Option<EvalSourceLocation>,
        attributes: Vec<EvalAttribute>,
        params: Vec<String>,
        parameter_attributes: Vec<Vec<EvalAttribute>>,
        parameter_types: Vec<Option<EvalParameterType>>,
        parameter_defaults: Vec<Option<EvalExpr>>,
        parameter_is_by_ref: Vec<bool>,
        parameter_is_variadic: Vec<bool>,
        return_type: Option<EvalParameterType>,
        /// Whether the declaration was `function &name()`, PHP's return-by-reference form.
        returns_by_ref: bool,
        body: Vec<EvalStmt>,
    },
    /// `declare(strict_types=1);` -- PHP's per-FILE scalar type-checking mode.
    ///
    /// A statement rather than a flag on the program because that is how it reaches the
    /// interpreter's coercion rules, and because php scopes it to the file whose calls it
    /// governs rather than to the callee.
    DeclareStrictTypes(bool),
    /// `declare(ticks=N);` or `declare(ticks=N) { … }` -- php's statement-counting hook.
    ///
    /// `body` is `None` for the statement form, which runs to the end of the enclosing scope, and
    /// `Some` for the block and `enddeclare` forms, which scope the directive to their body.
    DeclareTicks {
        every: i64,
        body: Option<Vec<EvalStmt>>,
    },
    /// Any other `declare(name=…)`, kept so its body still runs and its name can be reported.
    ///
    /// php never refuses one of these: `encoding` is accepted and ignored when Zend multibyte is
    /// off, and an unknown name is a warning the script continues past.
    DeclareDirective {
        name: String,
        body: Option<Vec<EvalStmt>>,
    },
    Goto(String),
    /// The source line the statements after it were written on.
    ///
    /// Emitted only when a whole FILE is parsed, which is the case that needs it: an included
    /// file's diagnostics, `debug_backtrace()` frames and `eval()`'d-code spellings all report a
    /// position, and without a per-statement marker that position stayed at the line the include
    /// itself began on. A fragment is a piece of a file whose position its caller already knows,
    /// and `__LINE__` is stamped by the lexer either way, so fragments carry no markers and every
    /// statement-shape expectation written against them is unaffected.
    SourceLine(i64),
    Global {
        vars: Vec<String>,
    },
    If {
        condition: EvalExpr,
        then_branch: Vec<EvalStmt>,
        else_branch: Vec<EvalStmt>,
    },
    Return(Option<EvalExpr>),
    ReferenceAssign {
        target: String,
        source: String,
    },
    /// Binds a variable to a non-variable lvalue source, as in `$x = &$this->p;`.
    ///
    /// `ReferenceAssign` covers the variable-to-variable case, where PHP aliases two scope
    /// names; this variant covers every other assignable source PHP accepts after `=&`
    /// (properties, static properties, array elements, and their dynamic-name forms).
    VarReferenceBind {
        target: String,
        source: EvalExpr,
    },
    PropertyReferenceBind {
        object: EvalExpr,
        property: String,
        source: String,
    },
    DynamicPropertyReferenceBind {
        object: EvalExpr,
        property: EvalExpr,
        source: String,
    },
    DynamicPropertySet {
        object: EvalExpr,
        property: EvalExpr,
        value: EvalExpr,
    },
    DynamicPropertyArrayAppend {
        object: EvalExpr,
        property: EvalExpr,
        value: EvalExpr,
    },
    DynamicPropertyArraySet {
        object: EvalExpr,
        property: EvalExpr,
        index: EvalExpr,
        op: Option<EvalBinOp>,
        value: EvalExpr,
    },
    DynamicPropertyCompoundAssign {
        object: EvalExpr,
        property: EvalExpr,
        op: EvalBinOp,
        value: EvalExpr,
    },
    DynamicPropertyIncDec {
        object: EvalExpr,
        property: EvalExpr,
        increment: bool,
    },
    PropertySet {
        object: EvalExpr,
        property: String,
        value: EvalExpr,
    },
    PropertyArrayAppend {
        object: EvalExpr,
        property: String,
        value: EvalExpr,
    },
    PropertyArraySet {
        object: EvalExpr,
        property: String,
        index: EvalExpr,
        op: Option<EvalBinOp>,
        value: EvalExpr,
    },
    PropertyCompoundAssign {
        object: EvalExpr,
        property: String,
        op: EvalBinOp,
        value: EvalExpr,
    },
    PropertyIncDec {
        object: EvalExpr,
        property: String,
        increment: bool,
    },
    StaticPropertySet {
        class_name: String,
        property: String,
        value: EvalExpr,
    },
    StaticPropertyReferenceBind {
        class_name: String,
        property: String,
        source: String,
    },
    StaticPropertyArrayAppend {
        class_name: String,
        property: String,
        value: EvalExpr,
    },
    StaticPropertyArraySet {
        class_name: String,
        property: String,
        index: EvalExpr,
        op: Option<EvalBinOp>,
        value: EvalExpr,
    },
    StaticPropertyIncDec {
        class_name: String,
        property: String,
        increment: bool,
    },
    DynamicStaticPropertySet {
        class_name: EvalExpr,
        property: String,
        value: EvalExpr,
    },
    DynamicStaticPropertyReferenceBind {
        class_name: EvalExpr,
        property: String,
        source: String,
    },
    DynamicStaticPropertyArrayAppend {
        class_name: EvalExpr,
        property: String,
        value: EvalExpr,
    },
    DynamicStaticPropertyArraySet {
        class_name: EvalExpr,
        property: String,
        index: EvalExpr,
        op: Option<EvalBinOp>,
        value: EvalExpr,
    },
    DynamicStaticPropertyIncDec {
        class_name: EvalExpr,
        property: String,
        increment: bool,
    },
    DynamicStaticPropertyNameSet {
        class_name: EvalExpr,
        property: EvalExpr,
        value: EvalExpr,
    },
    DynamicStaticPropertyNameReferenceBind {
        class_name: EvalExpr,
        property: EvalExpr,
        source: String,
    },
    DynamicStaticPropertyNameArrayAppend {
        class_name: EvalExpr,
        property: EvalExpr,
        value: EvalExpr,
    },
    DynamicStaticPropertyNameArraySet {
        class_name: EvalExpr,
        property: EvalExpr,
        index: EvalExpr,
        op: Option<EvalBinOp>,
        value: EvalExpr,
    },
    DynamicStaticPropertyNameIncDec {
        class_name: EvalExpr,
        property: EvalExpr,
        increment: bool,
    },
    StaticVar {
        name: String,
        init: EvalExpr,
    },
    StoreVar {
        name: String,
        value: EvalExpr,
    },
    Switch {
        expr: EvalExpr,
        cases: Vec<EvalSwitchCase>,
    },
    Throw(EvalExpr),
    Try {
        body: Vec<EvalStmt>,
        catches: Vec<EvalCatch>,
        finally_body: Vec<EvalStmt>,
    },
    UnsetArrayElement {
        array: EvalExpr,
        index: EvalExpr,
    },
    UnsetProperty {
        object: EvalExpr,
        property: String,
    },
    UnsetDynamicProperty {
        object: EvalExpr,
        property: EvalExpr,
    },
    UnsetStaticProperty {
        class_name: String,
        property: String,
    },
    UnsetDynamicStaticProperty {
        class_name: EvalExpr,
        property: String,
    },
    UnsetDynamicStaticPropertyName {
        class_name: EvalExpr,
        property: EvalExpr,
    },
    UnsetVar {
        name: String,
    },
    /// `unset($$name)` / `unset(${expr})`: the name is evaluated, then that variable is removed.
    UnsetDynamicVar {
        name: EvalExpr,
    },
    /// Collects cycles after every operand of one source-level unset has been removed.
    GcCollect,
    While {
        condition: EvalExpr,
        body: Vec<EvalStmt>,
    },
    Expr(EvalExpr),
}

impl EvalStmt {
    /// Builds a class declaration statement, sharing the class with everything that declares it.
    pub fn class_decl(class: EvalClass) -> Self {
        Self::ClassDecl(Arc::new(class))
    }
}

/// One `catch` block attached to an eval `try` statement.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct EvalCatch {
    pub class_names: Vec<String>,
    pub var_name: Option<String>,
    pub body: Vec<EvalStmt>,
}
