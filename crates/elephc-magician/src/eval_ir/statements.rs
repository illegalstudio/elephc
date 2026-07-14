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
#[derive(Debug, Clone, PartialEq)]
pub enum EvalStmt {
    ArrayAppendVar {
        name: String,
        value: EvalExpr,
    },
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
        body: Vec<EvalStmt>,
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
    While {
        condition: EvalExpr,
        body: Vec<EvalStmt>,
    },
    Expr(EvalExpr),
}

/// One `catch` block attached to an eval `try` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalCatch {
    pub class_names: Vec<String>,
    pub var_name: Option<String>,
    pub body: Vec<EvalStmt>,
}
