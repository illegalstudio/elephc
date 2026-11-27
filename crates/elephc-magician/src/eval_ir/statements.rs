//! Purpose:
//! Defines EvalIR statement and catch-clause variants.
//!
//! Called from:
//! - Statement parser and interpreter statement dispatcher.
//!
//! Key details:
//! - Statements encode explicit mutation and structured control flow without runtime ownership.

use super::*;

/// Dynamic eval statements that operate on a materialized activation scope.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum EvalStmt {
    GenericDecl(Box<EvalGenericTemplate>),
    TypedVar { name: String, declared: EvalParameterType, value: EvalExpr },
    ArrayAppendVar {
        name: String,
        value: EvalExpr,
    },
    ArraySetVar {
        name: String,
        index: EvalExpr,
        value: EvalExpr,
    },
    DimensionWrite {
        array: EvalExpr,
        index: Option<EvalExpr>,
        op: Option<EvalBinOp>,
        value: EvalExpr,
    },
    Break,
    Continue,
    /// `goto name;`: a jump to the label `name` of the same function or fragment.
    Goto(String),
    /// `name:` at statement position: the target of a `goto`.
    Label(String),
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
    ClassDecl(EvalClass),
    EnumDecl(EvalEnum),
    InterfaceDecl(EvalInterface),
    TraitDecl(EvalTrait),
    Foreach {
        array: EvalExpr,
        key_name: Option<String>,
        value_name: String,
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
        /// Whether the declaration is spelled `function &name()`.
        returns_by_ref: bool,
        body: Vec<EvalStmt>,
    },
    /// A top-level `const NAME = value;` declaration with its namespace-qualified name.
    ConstDecl {
        name: String,
        value: EvalExpr,
    },
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
    /// `$name++`, `$name--`, `++$name`, or `--$name` as a statement.
    ///
    /// Kept distinct from `StoreVar` because PHP leaves a null local unchanged on
    /// decrement instead of evaluating `null - 1`.
    IncDecVar {
        name: String,
        increment: bool,
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
    /// Collects cycles after every operand of one source-level unset has been removed.
    GcCollect,
    While {
        condition: EvalExpr,
        body: Vec<EvalStmt>,
    },
    Expr(EvalExpr),
}

impl EvalStmt {
    /// Returns whether a jump to `label` can resume at this statement: it is that label, or
    /// the label sits in one of its `if` branches or in its `try` body, at any depth.
    ///
    /// This is the ONE definition of where a `goto` may land. The parser validates every jump
    /// against it and the interpreter resolves every jump with it, so a jump the parser accepts
    /// is exactly a jump execution can complete. Loop and `switch` bodies are deliberately not
    /// searched: PHP forbids jumping into them. `catch` and `finally` bodies are not searched
    /// either: PHP forbids entering a `finally`, and entering a `catch` is refused here as it is
    /// by the ahead-of-time compiler. A label inside any of them is reachable only from inside,
    /// where its own statement list finds it.
    pub fn holds_jump_label(&self, label: &str) -> bool {
        match self {
            EvalStmt::Label(name) => name == label,
            EvalStmt::If {
                then_branch,
                else_branch,
                ..
            } => sequence_holds_jump_label(then_branch, label)
                || sequence_holds_jump_label(else_branch, label),
            EvalStmt::Try { body, .. } => sequence_holds_jump_label(body, label),
            _ => false,
        }
    }
}

/// Returns the index of the statement of `statements` a jump to `label` resumes at.
pub fn jump_label_position(statements: &[EvalStmt], label: &str) -> Option<usize> {
    statements.iter().position(|stmt| stmt.holds_jump_label(label))
}

/// Returns whether a jump to `label` can resume somewhere in `statements`.
pub fn sequence_holds_jump_label(statements: &[EvalStmt], label: &str) -> bool {
    jump_label_position(statements, label).is_some()
}

/// One `catch` block attached to an eval `try` statement.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct EvalCatch {
    pub class_names: Vec<String>,
    pub var_name: Option<String>,
    pub body: Vec<EvalStmt>,
}
