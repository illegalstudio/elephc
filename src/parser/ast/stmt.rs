//! Purpose:
//! Defines statement AST nodes for PHP programs and elefc statement-level extensions.
//! Carries declarations, control flow, includes, namespace/use statements, and source spans.
//!
//! Called from:
//! - `crate::parser::stmt`, `crate::parser::control`, and all statement-walking compiler passes.
//!
//! Key details:
//! - Statement variants form the main pass contract for resolver discovery, type checking, and codegen.

use crate::names::Name;
use crate::span::Span;

use super::{
    AttributeGroup, CType, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr,
    ExternField, ExternParam, PackedField, StaticReceiver, TraitUse, TypeExpr,
};

// --- Statements ---

#[derive(Debug, Clone)]
/// Statement AST node.
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
    /// PHP attributes attached to this statement. Only populated for
    /// declaration kinds (`ClassDecl`, `FunctionDecl`, etc.); the parser
    /// rejects attributes on non-declaration statements.
    pub attributes: Vec<AttributeGroup>,
}

impl Stmt {
    /// Creates a `Stmt` with the given kind and source span, with an empty attribute list.
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Stmt { kind, span, attributes: Vec::new() }
    }

    /// Creates a `Stmt` with the given kind, source span, and PHP attribute list.
    pub fn with_attributes(
        kind: StmtKind,
        span: Span,
        attributes: Vec<AttributeGroup>,
    ) -> Self {
        Stmt { kind, span, attributes }
    }
}

#[derive(Debug, Clone, PartialEq)]
/// Catch clause. Holds exception type names, optional variable name,
/// and the statements in the catch body.
pub struct CatchClause {
    pub exception_types: Vec<Name>,
    pub variable: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Kind of a use statement: class, function, or const.
pub enum UseKind {
    Class,
    Function,
    Const,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Use item. Represents a single imported name with optional alias.
pub struct UseItem {
    pub kind: UseKind,
    pub name: Name,
    pub alias: String,
}

#[derive(Debug, Clone, PartialEq)]
/// Statement kind enumerates all PHP and elefc statement forms.
pub enum StmtKind {
    Echo(Expr),
    Assign {
        name: String,
        value: Expr,
    },
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
    },
    IfDef {
        symbol: String,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    DoWhile {
        body: Vec<Stmt>,
        condition: Expr,
    },
    For {
        init: Option<Box<Stmt>>,
        condition: Option<Expr>,
        update: Option<Box<Stmt>>,
        body: Vec<Stmt>,
    },
    ArrayAssign {
        array: String,
        index: Expr,
        value: Expr,
    },
    NestedArrayAssign {
        target: Expr,
        value: Expr,
    },
    ArrayPush {
        array: String,
        value: Expr,
    },
    TypedAssign {
        type_expr: TypeExpr,
        name: String,
        value: Expr,
    },
    Foreach {
        array: Expr,
        key_var: Option<String>,
        value_var: String,
        value_by_ref: bool,
        body: Vec<Stmt>,
    },
    Switch {
        subject: Expr,
        cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
        default: Option<Vec<Stmt>>,
    },
    Include {
        path: Expr,
        once: bool,
        required: bool,
    },
    IncludeOnceMark {
        label: String,
    },
    IncludeOnceGuard {
        label: String,
        body: Vec<Stmt>,
    },
    Throw(Expr),
    Synthetic(Vec<Stmt>),
    Try {
        try_body: Vec<Stmt>,
        catches: Vec<CatchClause>,
        finally_body: Option<Vec<Stmt>>,
    },
    Break(usize),
    Continue(usize),
    ExprStmt(Expr),
    NamespaceDecl {
        name: Option<Name>,
    },
    NamespaceBlock {
        name: Option<Name>,
        body: Vec<Stmt>,
    },
    UseDecl {
        imports: Vec<UseItem>,
    },
    FunctionDecl {
        name: String,
        params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
        variadic: Option<String>,
        return_type: Option<TypeExpr>,
        body: Vec<Stmt>,
    },
    FunctionVariantGroup {
        name: String,
        variants: Vec<String>,
    },
    FunctionVariantMark {
        name: String,
        variant: String,
    },
    Return(Option<Expr>),
    ConstDecl {
        name: String,
        value: Expr,
    },
    ListUnpack {
        vars: Vec<String>,
        value: Expr,
    },
    Global {
        vars: Vec<String>,
    },
    StaticVar {
        name: String,
        init: Expr,
    },
    ClassDecl {
        name: String,
        extends: Option<Name>,
        implements: Vec<Name>,
        is_abstract: bool,
        is_final: bool,
        is_readonly_class: bool,
        trait_uses: Vec<TraitUse>,
        properties: Vec<ClassProperty>,
        methods: Vec<ClassMethod>,
        constants: Vec<ClassConst>,
    },
    EnumDecl {
        name: String,
        backing_type: Option<TypeExpr>,
        cases: Vec<EnumCaseDecl>,
    },
    PackedClassDecl {
        name: String,
        fields: Vec<PackedField>,
    },
    InterfaceDecl {
        name: String,
        extends: Vec<Name>,
        properties: Vec<ClassProperty>,
        methods: Vec<ClassMethod>,
        constants: Vec<ClassConst>,
    },
    TraitDecl {
        name: String,
        trait_uses: Vec<TraitUse>,
        properties: Vec<ClassProperty>,
        methods: Vec<ClassMethod>,
        constants: Vec<ClassConst>,
    },
    PropertyAssign {
        object: Box<Expr>,
        property: String,
        value: Expr,
    },
    StaticPropertyAssign {
        receiver: StaticReceiver,
        property: String,
        value: Expr,
    },
    StaticPropertyArrayPush {
        receiver: StaticReceiver,
        property: String,
        value: Expr,
    },
    StaticPropertyArrayAssign {
        receiver: StaticReceiver,
        property: String,
        index: Expr,
        value: Expr,
    },
    PropertyArrayPush {
        object: Box<Expr>,
        property: String,
        value: Expr,
    },
    PropertyArrayAssign {
        object: Box<Expr>,
        property: String,
        index: Expr,
        value: Expr,
    },
    ExternFunctionDecl {
        name: String,
        params: Vec<ExternParam>,
        return_type: CType,
        library: Option<String>,
    },
    ExternClassDecl {
        name: String,
        fields: Vec<ExternField>,
    },
    ExternGlobalDecl {
        name: String,
        c_type: CType,
    },
}

impl PartialEq for Stmt {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

#[allow(dead_code)] // Constructors used by test crate
impl Stmt {
    /// Creates an `Echo` statement with the given expression.
    pub fn echo(expr: Expr) -> Self {
        Self::new(StmtKind::Echo(expr), Span::dummy())
    }

    /// Creates an `Assign` statement for the given variable name and value expression.
    pub fn assign(name: impl Into<String>, value: Expr) -> Self {
        Self::new(
            StmtKind::Assign {
                name: name.into(),
                value,
            },
            Span::dummy(),
        )
    }
}

/// Type alias for a program as a vector of statements.
pub type Program = Vec<Stmt>;
