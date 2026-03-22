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
    PreIncrement(String),
    PostIncrement(String),
    PreDecrement(String),
    PostDecrement(String),
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
    ArrayLiteral(Vec<Expr>),
    ArrayAccess {
        array: Box<Expr>,
        index: Box<Expr>,
    },
    Ternary {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
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
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
}

// --- Statements ---

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
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
        value_var: String,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    ExprStmt(Expr),
    FunctionDecl {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
    },
    Return(Option<Expr>),
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
