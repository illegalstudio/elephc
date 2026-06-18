//! Purpose:
//! Defines the dynamic EvalIR used by runtime `eval()` fragments.
//! EvalIR models by-name variable operations and expression shape without
//! introducing an independent runtime value representation.
//!
//! Called from:
//! - `crate::parser::parse_fragment()`
//! - Future `crate::interpreter` execution.
//!
//! Key details:
//! - Runtime execution must turn constants into elephc runtime cells through
//!   value-bridge hooks; EvalIR constants are syntax data, not owned PHP values.

/// Parsed eval fragment lowered into dynamic by-name statements.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalProgram {
    source_len: usize,
    statements: Vec<EvalStmt>,
}

impl EvalProgram {
    /// Creates an EvalIR program for a source fragment and statement list.
    pub fn new(source_len: usize, statements: Vec<EvalStmt>) -> Self {
        Self {
            source_len,
            statements,
        }
    }

    /// Returns the byte length of the parsed eval fragment.
    pub const fn source_len(&self) -> usize {
        self.source_len
    }

    /// Returns the ordered EvalIR statements in source order.
    pub fn statements(&self) -> &[EvalStmt] {
        &self.statements
    }

    /// Consumes the program and returns its statement list.
    pub fn into_statements(self) -> Vec<EvalStmt> {
        self.statements
    }
}

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
        params: Vec<String>,
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
    PropertySet {
        object: EvalExpr,
        property: String,
        value: EvalExpr,
    },
    StaticPropertySet {
        class_name: String,
        property: String,
        value: EvalExpr,
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

/// Runtime user function declared by an eval fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalFunction {
    name: String,
    params: Vec<String>,
    body: Vec<EvalStmt>,
}

impl EvalFunction {
    /// Creates a dynamic eval function with source-order parameters and body.
    pub fn new(name: impl Into<String>, params: Vec<String>, body: Vec<EvalStmt>) -> Self {
        Self {
            name: name.into(),
            params,
            body,
        }
    }

    /// Returns the original source spelling of this eval-declared function name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns source-order parameter names without leading `$`.
    pub fn params(&self) -> &[String] {
        &self.params
    }

    /// Returns the dynamic EvalIR statements that form the function body.
    pub fn body(&self) -> &[EvalStmt] {
        &self.body
    }
}

/// Runtime interface declared by an eval fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalInterface {
    name: String,
    parents: Vec<String>,
    methods: Vec<EvalInterfaceMethod>,
}

impl EvalInterface {
    /// Creates a dynamic eval interface with optional parent interfaces and methods.
    pub fn new(
        name: impl Into<String>,
        parents: Vec<String>,
        methods: Vec<EvalInterfaceMethod>,
    ) -> Self {
        Self {
            name: name.into(),
            parents,
            methods,
        }
    }

    /// Returns the original source spelling of this eval-declared interface name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns interface names extended directly by this eval interface.
    pub fn parents(&self) -> &[String] {
        &self.parents
    }

    /// Returns method signatures declared directly by this eval interface.
    pub fn methods(&self) -> &[EvalInterfaceMethod] {
        &self.methods
    }
}

/// Method signature metadata for a runtime eval interface.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalInterfaceMethod {
    name: String,
    params: Vec<String>,
}

impl EvalInterfaceMethod {
    /// Creates one dynamic eval interface method signature.
    pub fn new(name: impl Into<String>, params: Vec<String>) -> Self {
        Self {
            name: name.into(),
            params,
        }
    }

    /// Returns the PHP-visible method name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns source-order parameter names without leading `$`.
    pub fn params(&self) -> &[String] {
        &self.params
    }
}

/// Runtime class declared by an eval fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalClass {
    name: String,
    is_abstract: bool,
    is_final: bool,
    parent: Option<String>,
    interfaces: Vec<String>,
    traits: Vec<String>,
    properties: Vec<EvalClassProperty>,
    methods: Vec<EvalClassMethod>,
}

impl EvalClass {
    /// Creates a dynamic eval class with public properties and methods, and no relations.
    pub fn new(
        name: impl Into<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_modifiers(name, false, false, None, Vec::new(), properties, methods)
    }

    /// Creates a dynamic eval class with optional parent and implemented interfaces.
    pub fn with_relations(
        name: impl Into<String>,
        parent: Option<String>,
        interfaces: Vec<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_modifiers(name, false, false, parent, interfaces, properties, methods)
    }

    /// Creates a dynamic eval class with optional modifiers, parent, and interfaces.
    pub fn with_modifiers(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_modifiers_and_traits(
            name,
            is_abstract,
            is_final,
            parent,
            interfaces,
            Vec::new(),
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with optional modifiers, relations, and trait uses.
    pub fn with_modifiers_and_traits(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self {
            name: name.into(),
            is_abstract,
            is_final,
            parent,
            interfaces,
            traits,
            properties,
            methods,
        }
    }

    /// Returns the original source spelling of this eval-declared class name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns whether this eval-declared class was declared `abstract`.
    pub const fn is_abstract(&self) -> bool {
        self.is_abstract
    }

    /// Returns whether this eval-declared class was declared `final`.
    pub const fn is_final(&self) -> bool {
        self.is_final
    }

    /// Returns the parent class name declared by this eval class, when present.
    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    /// Returns interface names implemented directly by this eval class.
    pub fn interfaces(&self) -> &[String] {
        &self.interfaces
    }

    /// Returns trait names used directly by this eval class.
    pub fn traits(&self) -> &[String] {
        &self.traits
    }

    /// Returns public properties declared directly by this eval class.
    pub fn properties(&self) -> &[EvalClassProperty] {
        &self.properties
    }

    /// Returns public methods declared directly by this eval class.
    pub fn methods(&self) -> &[EvalClassMethod] {
        &self.methods
    }

    /// Returns a public method by PHP case-insensitive method name.
    pub fn method(&self, name: &str) -> Option<&EvalClassMethod> {
        self.methods()
            .iter()
            .find(|method| method.name().eq_ignore_ascii_case(name))
    }
}

/// Runtime trait declared by an eval fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalTrait {
    name: String,
    properties: Vec<EvalClassProperty>,
    methods: Vec<EvalClassMethod>,
}

impl EvalTrait {
    /// Creates a dynamic eval trait with public properties and methods.
    pub fn new(
        name: impl Into<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self {
            name: name.into(),
            properties,
            methods,
        }
    }

    /// Returns the original source spelling of this eval-declared trait name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns public properties declared directly by this eval trait.
    pub fn properties(&self) -> &[EvalClassProperty] {
        &self.properties
    }

    /// Returns public methods declared directly by this eval trait.
    pub fn methods(&self) -> &[EvalClassMethod] {
        &self.methods
    }
}

/// Public property metadata for a runtime eval class.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalClassProperty {
    name: String,
    visibility: EvalVisibility,
    is_static: bool,
    default: Option<EvalExpr>,
}

impl EvalClassProperty {
    /// Creates a public eval class property with an optional initializer.
    pub fn new(name: impl Into<String>, default: Option<EvalExpr>) -> Self {
        Self::with_visibility(name, EvalVisibility::Public, default)
    }

    /// Creates an eval class property with explicit PHP visibility.
    pub fn with_visibility(
        name: impl Into<String>,
        visibility: EvalVisibility,
        default: Option<EvalExpr>,
    ) -> Self {
        Self::with_visibility_and_static(name, visibility, false, default)
    }

    /// Creates an eval class property with explicit PHP visibility and static metadata.
    pub fn with_visibility_and_static(
        name: impl Into<String>,
        visibility: EvalVisibility,
        is_static: bool,
        default: Option<EvalExpr>,
    ) -> Self {
        Self {
            name: name.into(),
            visibility,
            is_static,
            default,
        }
    }

    /// Returns the PHP-visible property name without `$`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the PHP visibility declared for this property.
    pub const fn visibility(&self) -> EvalVisibility {
        self.visibility
    }

    /// Returns whether this property was declared `static`.
    pub const fn is_static(&self) -> bool {
        self.is_static
    }

    /// Returns the property initializer expression, when one was declared.
    pub fn default(&self) -> Option<&EvalExpr> {
        self.default.as_ref()
    }
}

/// PHP visibility for eval-declared object members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalVisibility {
    Public,
    Protected,
    Private,
}

/// Public method metadata for a runtime eval class.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalClassMethod {
    name: String,
    visibility: EvalVisibility,
    is_static: bool,
    is_abstract: bool,
    is_final: bool,
    params: Vec<String>,
    body: Vec<EvalStmt>,
}

impl EvalClassMethod {
    /// Creates a public eval class method with source-order parameters and body.
    pub fn new(name: impl Into<String>, params: Vec<String>, body: Vec<EvalStmt>) -> Self {
        Self::with_modifiers(name, false, false, params, body)
    }

    /// Creates a public eval class method with optional abstract/final modifiers.
    pub fn with_modifiers(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        params: Vec<String>,
        body: Vec<EvalStmt>,
    ) -> Self {
        Self::with_visibility_and_modifiers(
            name,
            EvalVisibility::Public,
            false,
            is_abstract,
            is_final,
            params,
            body,
        )
    }

    /// Creates an eval class method with explicit visibility and optional modifiers.
    pub fn with_visibility_and_modifiers(
        name: impl Into<String>,
        visibility: EvalVisibility,
        is_static: bool,
        is_abstract: bool,
        is_final: bool,
        params: Vec<String>,
        body: Vec<EvalStmt>,
    ) -> Self {
        Self {
            name: name.into(),
            visibility,
            is_static,
            is_abstract,
            is_final,
            params,
            body,
        }
    }

    /// Returns the PHP-visible method name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the PHP visibility declared for this method.
    pub const fn visibility(&self) -> EvalVisibility {
        self.visibility
    }

    /// Returns whether this method was declared `static`.
    pub const fn is_static(&self) -> bool {
        self.is_static
    }

    /// Returns whether this eval-declared method was declared `abstract`.
    pub const fn is_abstract(&self) -> bool {
        self.is_abstract
    }

    /// Returns whether this eval-declared method was declared `final`.
    pub const fn is_final(&self) -> bool {
        self.is_final
    }

    /// Returns source-order parameter names without leading `$`.
    pub fn params(&self) -> &[String] {
        &self.params
    }

    /// Returns the dynamic EvalIR statements that form the method body.
    pub fn body(&self) -> &[EvalStmt] {
        &self.body
    }
}

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
    Const(EvalConst),
    ConstFetch(String),
    DynamicCall {
        callee: Box<EvalExpr>,
        args: Vec<EvalCallArg>,
    },
    Include {
        path: Box<EvalExpr>,
        required: bool,
        once: bool,
    },
    LoadVar(String),
    Match {
        subject: Box<EvalExpr>,
        arms: Vec<EvalMatchArm>,
        default: Option<Box<EvalExpr>>,
    },
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
    Magic(EvalMagicConst),
    NewObject {
        class_name: String,
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
    NullCoalesce {
        value: Box<EvalExpr>,
        default: Box<EvalExpr>,
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
    KeyValue { key: EvalExpr, value: EvalExpr },
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

/// Unary operations supported by the initial EvalIR parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalUnaryOp {
    Plus,
    Negate,
    LogicalNot,
    BitNot,
}
