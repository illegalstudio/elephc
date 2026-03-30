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
    NullCoalesce {
        value: Box<Expr>,
        default: Box<Expr>,
    },
    PreIncrement(String),
    PostIncrement(String),
    PreDecrement(String),
    PostDecrement(String),
    FunctionCall {
        name: String,
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
    ConstRef(String),
    NewObject {
        class_name: String,
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
    This,
    PtrCast {
        target_type: String,
        expr: Box<Expr>,
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
    Named(String),
    Self_,
    Static,
    Parent,
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
    pub exception_types: Vec<String>,
    pub variable: String,
    pub body: Vec<Stmt>,
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
        extends: Option<String>,
        implements: Vec<String>,
        is_abstract: bool,
        trait_uses: Vec<TraitUse>,
        properties: Vec<ClassProperty>,
        methods: Vec<ClassMethod>,
    },
    InterfaceDecl {
        name: String,
        extends: Vec<String>,
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

// --- OOP ---

#[derive(Debug, Clone, PartialEq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

#[derive(Debug, Clone)]
pub struct TraitUse {
    pub trait_names: Vec<String>,
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
        trait_name: Option<String>,
        method: String,
        alias: Option<String>,
        visibility: Option<Visibility>,
    },
    InsteadOf {
        trait_name: Option<String>,
        method: String,
        instead_of: Vec<String>,
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
