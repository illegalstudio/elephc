use crate::names::Name;
use crate::span::Span;

// --- Expressions ---

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
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
    BoolLiteral(bool),
    Null,
    Negate(Box<Expr>),
    Not(Box<Expr>),
    BitNot(Box<Expr>),
    Throw(Box<Expr>),
    NullCoalesce {
        value: Box<Expr>,
        default: Box<Expr>,
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
    Cast {
        target: CastType,
        expr: Box<Expr>,
    },
    Closure {
        params: Vec<(String, Option<Expr>, bool)>,
        variadic: Option<String>,
        body: Vec<Stmt>,
        is_arrow: bool,
        captures: Vec<String>,
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
    PropertyAccess {
        object: Box<Expr>,
        property: String,
    },
    MethodCall {
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
}

#[derive(Debug, Clone, PartialEq)]
pub enum CastType {
    Int,
    Float,
    String,
    Bool,
    Array,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StaticReceiver {
    Named(Name),
    Self_,
    Static,
    Parent,
}

#[derive(Debug, Clone, PartialEq)]
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
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

#[allow(dead_code)] // Constructors used by test crate
impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn string_lit(s: impl Into<String>) -> Self {
        Self::new(ExprKind::StringLiteral(s.into()), Span::dummy())
    }

    pub fn int_lit(n: i64) -> Self {
        Self::new(ExprKind::IntLiteral(n), Span::dummy())
    }

    pub fn float_lit(f: f64) -> Self {
        Self::new(ExprKind::FloatLiteral(f), Span::dummy())
    }

    pub fn var(name: impl Into<String>) -> Self {
        Self::new(ExprKind::Variable(name.into()), Span::dummy())
    }

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

    pub fn negate(inner: Expr) -> Self {
        Self::new(ExprKind::Negate(Box::new(inner)), Span::dummy())
    }
}

// --- Operators ---

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Concat,
    Eq,
    NotEq,
    StrictEq,
    StrictNotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Pow,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    Spaceship,
    NullCoalesce,
}

// --- Statements ---

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CatchClause {
    pub exception_types: Vec<Name>,
    pub variable: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseKind {
    Class,
    Function,
    Const,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseItem {
    pub kind: UseKind,
    pub name: Name,
    pub alias: String,
}

#[derive(Debug, Clone, PartialEq)]
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
        body: Vec<Stmt>,
    },
    Switch {
        subject: Expr,
        cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
        default: Option<Vec<Stmt>>,
    },
    Include {
        path: String,
        once: bool,
        required: bool,
    },
    Throw(Expr),
    Try {
        try_body: Vec<Stmt>,
        catches: Vec<CatchClause>,
        finally_body: Option<Vec<Stmt>>,
    },
    Break,
    Continue,
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
        params: Vec<(String, Option<Expr>, bool)>,
        variadic: Option<String>,
        body: Vec<Stmt>,
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
        is_readonly_class: bool,
        trait_uses: Vec<TraitUse>,
        properties: Vec<ClassProperty>,
        methods: Vec<ClassMethod>,
    },
    PackedClassDecl {
        name: String,
        fields: Vec<PackedField>,
    },
    InterfaceDecl {
        name: String,
        extends: Vec<Name>,
        methods: Vec<ClassMethod>,
    },
    TraitDecl {
        name: String,
        trait_uses: Vec<TraitUse>,
        properties: Vec<ClassProperty>,
        methods: Vec<ClassMethod>,
    },
    PropertyAssign {
        object: Box<Expr>,
        property: String,
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
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn echo(expr: Expr) -> Self {
        Self::new(StmtKind::Echo(expr), Span::dummy())
    }

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

pub type Program = Vec<Stmt>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    Int,
    Float,
    Bool,
    Ptr(Option<Name>),
    Buffer(Box<TypeExpr>),
    Named(Name),
}

// --- FFI ---

/// C type annotation for extern declarations
#[derive(Debug, Clone, PartialEq)]
pub enum CType {
    Int,
    Float,
    Str,        // char* (null-terminated)
    Bool,
    Void,
    Ptr,                    // opaque void*
    TypedPtr(String),       // ptr<ClassName>
    Callable,               // function pointer
}

/// Parameter in an extern function declaration
#[derive(Debug, Clone, PartialEq)]
pub struct ExternParam {
    pub name: String,
    pub c_type: CType,
}

/// Field in an extern class (C struct) declaration
#[derive(Debug, Clone, PartialEq)]
pub struct ExternField {
    pub name: String,
    pub c_type: CType,
}

#[derive(Debug, Clone)]
pub struct PackedField {
    pub name: String,
    pub type_expr: TypeExpr,
    pub span: Span,
}

impl PartialEq for PackedField {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.type_expr == other.type_expr
    }
}

// --- OOP ---

#[derive(Debug, Clone, PartialEq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

#[derive(Debug, Clone)]
pub struct TraitUse {
    pub trait_names: Vec<Name>,
    pub adaptations: Vec<TraitAdaptation>,
    // Used for trait-flattening diagnostics.
    pub span: Span,
}

impl PartialEq for TraitUse {
    fn eq(&self, other: &Self) -> bool {
        self.trait_names == other.trait_names && self.adaptations == other.adaptations
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TraitAdaptation {
    Alias {
        trait_name: Option<Name>,
        method: String,
        alias: Option<String>,
        visibility: Option<Visibility>,
    },
    InsteadOf {
        trait_name: Option<Name>,
        method: String,
        instead_of: Vec<Name>,
    },
}

#[derive(Debug, Clone)]
pub struct ClassProperty {
    pub name: String,
    pub visibility: Visibility,
    pub readonly: bool,
    pub default: Option<Expr>,
    #[allow(dead_code)] // Used for error reporting in future phases
    pub span: Span,
}

impl PartialEq for ClassProperty {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.visibility == other.visibility
            && self.readonly == other.readonly
    }
}

#[derive(Debug, Clone)]
pub struct ClassMethod {
    pub name: String,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_abstract: bool,
    pub has_body: bool,
    pub params: Vec<(String, Option<Expr>, bool)>,
    pub variadic: Option<String>,
    pub body: Vec<Stmt>,
    #[allow(dead_code)] // Used for error reporting in future phases
    pub span: Span,
}

impl PartialEq for ClassMethod {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.visibility == other.visibility
            && self.is_static == other.is_static
            && self.is_abstract == other.is_abstract
            && self.has_body == other.has_body
    }
}
