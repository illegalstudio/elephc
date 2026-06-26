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

/// One source range inside the current eval fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalSourceLocation {
    start_line: i64,
    end_line: i64,
}

impl EvalSourceLocation {
    /// Creates a source range using one-based eval-fragment line numbers.
    pub const fn new(start_line: i64, end_line: i64) -> Self {
        Self {
            start_line,
            end_line,
        }
    }

    /// Creates a single-line source range.
    pub const fn single_line(line: i64) -> Self {
        Self::new(line, line)
    }

    /// Returns the one-based line where the declaration starts.
    pub const fn start_line(&self) -> i64 {
        self.start_line
    }

    /// Returns the one-based line where the declaration ends.
    pub const fn end_line(&self) -> i64 {
        self.end_line
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
#[derive(Debug, Clone)]
pub struct EvalFunction {
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
}

impl PartialEq for EvalFunction {
    /// Compares function metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.attributes == other.attributes
            && self.params == other.params
            && self.parameter_attributes == other.parameter_attributes
            && self.parameter_types == other.parameter_types
            && self.parameter_defaults == other.parameter_defaults
            && self.parameter_is_by_ref == other.parameter_is_by_ref
            && self.parameter_is_variadic == other.parameter_is_variadic
            && self.return_type == other.return_type
            && self.body == other.body
    }
}

impl EvalFunction {
    /// Creates a dynamic eval function with source-order parameters and body.
    pub fn new(name: impl Into<String>, params: Vec<String>, body: Vec<EvalStmt>) -> Self {
        let parameter_attributes = vec![Vec::new(); params.len()];
        let parameter_types = vec![None; params.len()];
        let parameter_defaults = vec![None; params.len()];
        let parameter_is_by_ref = vec![false; params.len()];
        let parameter_is_variadic = vec![false; params.len()];
        Self {
            name: name.into(),
            source_location: None,
            attributes: Vec::new(),
            params,
            parameter_attributes,
            parameter_types,
            parameter_defaults,
            parameter_is_by_ref,
            parameter_is_variadic,
            return_type: None,
            body,
        }
    }

    /// Returns a copy of this function with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns a copy of this function with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns a copy of this function with source-order parameter attributes.
    pub fn with_parameter_attributes(
        mut self,
        parameter_attributes: Vec<Vec<EvalAttribute>>,
    ) -> Self {
        self.parameter_attributes = parameter_attributes;
        self
    }

    /// Returns a copy of this function with source-order parameter type metadata.
    pub fn with_parameter_types(mut self, parameter_types: Vec<Option<EvalParameterType>>) -> Self {
        self.parameter_types = parameter_types;
        self
    }

    /// Returns a copy of this function with source-order default expressions.
    pub fn with_parameter_defaults(mut self, parameter_defaults: Vec<Option<EvalExpr>>) -> Self {
        self.parameter_defaults = parameter_defaults;
        self
    }

    /// Returns a copy of this function with source-order by-reference flags.
    pub fn with_parameter_by_ref_flags(mut self, parameter_is_by_ref: Vec<bool>) -> Self {
        self.parameter_is_by_ref = parameter_is_by_ref;
        self
    }

    /// Returns a copy of this function with source-order variadic flags.
    pub fn with_parameter_variadic_flags(mut self, parameter_is_variadic: Vec<bool>) -> Self {
        self.parameter_is_variadic = parameter_is_variadic;
        self
    }

    /// Returns a copy of this function with retained return type metadata.
    pub fn with_return_type(mut self, return_type: Option<EvalParameterType>) -> Self {
        self.return_type = return_type;
        self
    }

    /// Returns the original source spelling of this eval-declared function name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns attributes declared directly on this eval function.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns source-order parameter names without leading `$`.
    pub fn params(&self) -> &[String] {
        &self.params
    }

    /// Returns source-order parameter attributes.
    pub fn parameter_attributes(&self) -> &[Vec<EvalAttribute>] {
        &self.parameter_attributes
    }

    /// Returns source-order parameter type metadata.
    pub fn parameter_types(&self) -> &[Option<EvalParameterType>] {
        &self.parameter_types
    }

    /// Returns default expressions declared for each source-order parameter.
    pub fn parameter_defaults(&self) -> &[Option<EvalExpr>] {
        &self.parameter_defaults
    }

    /// Returns source-order flags for whether each parameter was declared by reference.
    pub fn parameter_is_by_ref(&self) -> &[bool] {
        &self.parameter_is_by_ref
    }

    /// Returns source-order flags for whether each parameter was declared variadic.
    pub fn parameter_is_variadic(&self) -> &[bool] {
        &self.parameter_is_variadic
    }

    /// Returns retained return type metadata, if the function declared one.
    pub const fn return_type(&self) -> Option<&EvalParameterType> {
        self.return_type.as_ref()
    }

    /// Returns the dynamic EvalIR statements that form the function body.
    pub fn body(&self) -> &[EvalStmt] {
        &self.body
    }
}

/// One supported eval method parameter type atom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalParameterTypeVariant {
    Array,
    Bool,
    Callable,
    Class(String),
    Float,
    Int,
    Iterable,
    Mixed,
    Never,
    Object,
    String,
    Void,
}

/// How multiple eval parameter type atoms combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalParameterTypeKind {
    Union,
    Intersection,
}

/// Type metadata retained for one eval method parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalParameterType {
    variants: Vec<EvalParameterTypeVariant>,
    allows_null: bool,
    kind: EvalParameterTypeKind,
}

impl EvalParameterType {
    /// Creates one eval method parameter type from union variants and nullability.
    pub fn new(variants: Vec<EvalParameterTypeVariant>, allows_null: bool) -> Self {
        Self {
            variants,
            allows_null,
            kind: EvalParameterTypeKind::Union,
        }
    }

    /// Creates one eval method parameter type from intersection variants.
    pub fn intersection(variants: Vec<EvalParameterTypeVariant>) -> Self {
        Self {
            variants,
            allows_null: false,
            kind: EvalParameterTypeKind::Intersection,
        }
    }

    /// Returns the non-null type atoms in source order.
    pub fn variants(&self) -> &[EvalParameterTypeVariant] {
        &self.variants
    }

    /// Returns whether the type explicitly accepts PHP null.
    pub const fn allows_null(&self) -> bool {
        self.allows_null
    }

    /// Returns whether all variants must match the value.
    pub const fn is_intersection(&self) -> bool {
        matches!(self.kind, EvalParameterTypeKind::Intersection)
    }
}

/// Literal attribute argument metadata retained by eval declarations.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalAttributeArg {
    String(String),
    Int(i64),
    Float(u64),
    Bool(bool),
    Null,
    Array(Vec<EvalAttributeArg>),
    Named {
        name: String,
        value: Box<EvalAttributeArg>,
    },
}

impl EvalAttributeArg {
    /// Returns the PHP named-argument key when this attribute arg is named.
    pub fn name(&self) -> Option<&str> {
        match self {
            EvalAttributeArg::Named { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Returns the scalar payload, unwrapping a named-argument wrapper.
    pub fn value(&self) -> &EvalAttributeArg {
        match self {
            EvalAttributeArg::Named { value, .. } => value,
            _ => self,
        }
    }
}

/// Attribute metadata retained for eval class-like declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalAttribute {
    name: String,
    args: Option<Vec<EvalAttributeArg>>,
}

impl EvalAttribute {
    /// Creates one eval attribute metadata entry.
    pub fn new(name: impl Into<String>, args: Option<Vec<EvalAttributeArg>>) -> Self {
        Self {
            name: name.into(),
            args,
        }
    }

    /// Returns the resolved PHP-visible attribute class name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns supported literal positional args, or `None` for unsupported metadata.
    pub fn args(&self) -> Option<&[EvalAttributeArg]> {
        self.args.as_deref()
    }
}

/// Runtime enum declared by an eval fragment.
#[derive(Debug, Clone)]
pub struct EvalEnum {
    name: String,
    source_location: Option<EvalSourceLocation>,
    backing_type: Option<EvalEnumBackingType>,
    interfaces: Vec<String>,
    attributes: Vec<EvalAttribute>,
    traits: Vec<String>,
    trait_adaptations: Vec<EvalTraitAdaptation>,
    cases: Vec<EvalEnumCase>,
    constants: Vec<EvalClassConstant>,
    methods: Vec<EvalClassMethod>,
}

impl PartialEq for EvalEnum {
    /// Compares enum metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.backing_type == other.backing_type
            && self.interfaces == other.interfaces
            && self.attributes == other.attributes
            && self.traits == other.traits
            && self.trait_adaptations == other.trait_adaptations
            && self.cases == other.cases
            && self.constants == other.constants
            && self.methods == other.methods
    }
}

impl EvalEnum {
    /// Creates a dynamic eval enum with cases and optional backing type.
    pub fn new(
        name: impl Into<String>,
        backing_type: Option<EvalEnumBackingType>,
        cases: Vec<EvalEnumCase>,
    ) -> Self {
        Self::with_members(
            name,
            backing_type,
            Vec::new(),
            cases,
            Vec::new(),
            Vec::new(),
        )
    }

    /// Creates a dynamic eval enum with interfaces, cases, constants, and methods.
    pub fn with_members(
        name: impl Into<String>,
        backing_type: Option<EvalEnumBackingType>,
        interfaces: Vec<String>,
        cases: Vec<EvalEnumCase>,
        constants: Vec<EvalClassConstant>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_members_traits_adaptations(
            name,
            backing_type,
            interfaces,
            cases,
            constants,
            methods,
            Vec::new(),
            Vec::new(),
        )
    }

    /// Creates a dynamic eval enum with traits, adaptations, cases, constants, and methods.
    pub fn with_members_traits_adaptations(
        name: impl Into<String>,
        backing_type: Option<EvalEnumBackingType>,
        interfaces: Vec<String>,
        cases: Vec<EvalEnumCase>,
        constants: Vec<EvalClassConstant>,
        methods: Vec<EvalClassMethod>,
        traits: Vec<String>,
        trait_adaptations: Vec<EvalTraitAdaptation>,
    ) -> Self {
        Self {
            name: name.into(),
            source_location: None,
            backing_type,
            interfaces,
            attributes: Vec::new(),
            traits,
            trait_adaptations,
            cases,
            constants,
            methods,
        }
    }

    /// Returns a copy of this enum with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns a copy of this enum with class-like attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns the original source spelling of this eval-declared enum name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns the optional scalar backing type for this enum.
    pub const fn backing_type(&self) -> Option<EvalEnumBackingType> {
        self.backing_type
    }

    /// Returns interface names implemented directly by this eval enum.
    pub fn interfaces(&self) -> &[String] {
        &self.interfaces
    }

    /// Returns attributes declared directly on this eval enum.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns trait names used directly by this eval enum.
    pub fn traits(&self) -> &[String] {
        &self.traits
    }

    /// Returns trait adaptations declared directly by this eval enum.
    pub fn trait_adaptations(&self) -> &[EvalTraitAdaptation] {
        &self.trait_adaptations
    }

    /// Returns cases declared directly by this eval enum.
    pub fn cases(&self) -> &[EvalEnumCase] {
        &self.cases
    }

    /// Returns one enum case by PHP case-sensitive case name.
    pub fn case(&self, name: &str) -> Option<&EvalEnumCase> {
        self.cases().iter().find(|case| case.name() == name)
    }

    /// Returns constants declared directly by this eval enum.
    pub fn constants(&self) -> &[EvalClassConstant] {
        &self.constants
    }

    /// Returns methods declared directly by this eval enum.
    pub fn methods(&self) -> &[EvalClassMethod] {
        &self.methods
    }

    /// Builds class-shaped metadata used for enum method and relation dispatch.
    pub fn as_class_metadata(&self) -> EvalClass {
        EvalClass::with_modifiers_traits_adaptations_and_constants(
            self.name.clone(),
            false,
            true,
            None,
            self.interfaces.clone(),
            self.traits.clone(),
            self.trait_adaptations.clone(),
            self.constants.clone(),
            Vec::new(),
            self.methods.clone(),
        )
        .with_attributes(self.attributes.clone())
        .with_source_location_option(self.source_location)
    }
}

/// Scalar backing type for a runtime eval enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalEnumBackingType {
    Int,
    String,
}

/// One case declared by a runtime eval enum.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalEnumCase {
    name: String,
    attributes: Vec<EvalAttribute>,
    value: Option<EvalExpr>,
}

impl EvalEnumCase {
    /// Creates an eval enum case with an optional backing value expression.
    pub fn new(name: impl Into<String>, value: Option<EvalExpr>) -> Self {
        Self {
            name: name.into(),
            attributes: Vec::new(),
            value,
        }
    }

    /// Returns a copy of this enum case with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns the PHP-visible enum case name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns attributes declared directly on this enum case.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns the optional backing value expression.
    pub fn value(&self) -> Option<&EvalExpr> {
        self.value.as_ref()
    }
}

/// Runtime interface declared by an eval fragment.
#[derive(Debug, Clone)]
pub struct EvalInterface {
    name: String,
    source_location: Option<EvalSourceLocation>,
    parents: Vec<String>,
    attributes: Vec<EvalAttribute>,
    constants: Vec<EvalClassConstant>,
    properties: Vec<EvalInterfaceProperty>,
    methods: Vec<EvalInterfaceMethod>,
}

impl PartialEq for EvalInterface {
    /// Compares interface metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.parents == other.parents
            && self.attributes == other.attributes
            && self.constants == other.constants
            && self.properties == other.properties
            && self.methods == other.methods
    }
}

impl EvalInterface {
    /// Creates a dynamic eval interface with optional parent interfaces and methods.
    pub fn new(
        name: impl Into<String>,
        parents: Vec<String>,
        methods: Vec<EvalInterfaceMethod>,
    ) -> Self {
        Self::with_constants(name, parents, Vec::new(), methods)
    }

    /// Creates a dynamic eval interface with optional parent interfaces, constants, and methods.
    pub fn with_constants(
        name: impl Into<String>,
        parents: Vec<String>,
        constants: Vec<EvalClassConstant>,
        methods: Vec<EvalInterfaceMethod>,
    ) -> Self {
        Self::with_constants_and_properties(name, parents, constants, Vec::new(), methods)
    }

    /// Creates a dynamic eval interface with constants, property contracts, and methods.
    pub fn with_constants_and_properties(
        name: impl Into<String>,
        parents: Vec<String>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalInterfaceProperty>,
        methods: Vec<EvalInterfaceMethod>,
    ) -> Self {
        Self {
            name: name.into(),
            source_location: None,
            parents,
            attributes: Vec::new(),
            constants,
            properties,
            methods,
        }
    }

    /// Returns a copy of this interface with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns a copy of this interface with class-like attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns the original source spelling of this eval-declared interface name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns interface names extended directly by this eval interface.
    pub fn parents(&self) -> &[String] {
        &self.parents
    }

    /// Returns attributes declared directly on this eval interface.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns constants declared directly by this eval interface.
    pub fn constants(&self) -> &[EvalClassConstant] {
        &self.constants
    }

    /// Returns one interface constant by PHP case-sensitive constant name.
    pub fn constant(&self, name: &str) -> Option<&EvalClassConstant> {
        self.constants()
            .iter()
            .find(|constant| constant.name() == name)
    }

    /// Returns property hook contracts declared directly by this eval interface.
    pub fn properties(&self) -> &[EvalInterfaceProperty] {
        &self.properties
    }

    /// Returns method signatures declared directly by this eval interface.
    pub fn methods(&self) -> &[EvalInterfaceMethod] {
        &self.methods
    }
}

/// Property hook contract metadata for a runtime eval interface.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalInterfaceProperty {
    name: String,
    attributes: Vec<EvalAttribute>,
    property_type: Option<EvalParameterType>,
    set_visibility: Option<EvalVisibility>,
    requires_get: bool,
    requires_set: bool,
}

impl EvalInterfaceProperty {
    /// Creates one eval interface property contract.
    pub fn new(name: impl Into<String>, requires_get: bool, requires_set: bool) -> Self {
        Self {
            name: name.into(),
            attributes: Vec::new(),
            property_type: None,
            set_visibility: None,
            requires_get,
            requires_set,
        }
    }

    /// Returns a copy of this interface property with retained type metadata.
    pub fn with_type(mut self, property_type: Option<EvalParameterType>) -> Self {
        self.property_type = property_type;
        self
    }

    /// Returns a copy of this interface property with PHP asymmetric write visibility metadata.
    pub const fn with_set_visibility(mut self, set_visibility: Option<EvalVisibility>) -> Self {
        self.set_visibility = set_visibility;
        self
    }

    /// Returns a copy of this interface property with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns the PHP-visible property name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns attributes declared directly on this interface property.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns retained PHP type metadata for this interface property contract.
    pub fn property_type(&self) -> Option<&EvalParameterType> {
        self.property_type.as_ref()
    }

    /// Returns the PHP asymmetric write visibility declared by this contract, if any.
    pub const fn set_visibility(&self) -> Option<EvalVisibility> {
        self.set_visibility
    }

    /// Returns the visibility required for writes by this property contract.
    pub const fn write_visibility(&self) -> EvalVisibility {
        match self.set_visibility {
            Some(visibility) => visibility,
            None => EvalVisibility::Public,
        }
    }

    /// Returns whether the interface requires the property to be readable.
    pub const fn requires_get(&self) -> bool {
        self.requires_get
    }

    /// Returns whether the interface requires the property to be writable.
    pub const fn requires_set(&self) -> bool {
        self.requires_set
    }

    /// Returns a merged contract containing either side's get/set requirements.
    pub fn merged_with(&self, other: &Self) -> Self {
        Self {
            name: self.name.clone(),
            attributes: self.attributes.clone(),
            property_type: self
                .property_type
                .clone()
                .or_else(|| other.property_type.clone()),
            set_visibility: merge_eval_property_set_visibility(
                self.set_visibility,
                other.set_visibility,
            ),
            requires_get: self.requires_get || other.requires_get,
            requires_set: self.requires_set || other.requires_set,
        }
    }
}

/// Merges interface property set-visibility contracts by keeping the stricter write requirement.
const fn merge_eval_property_set_visibility(
    left: Option<EvalVisibility>,
    right: Option<EvalVisibility>,
) -> Option<EvalVisibility> {
    let left = match left {
        Some(visibility) => visibility,
        None => EvalVisibility::Public,
    };
    let right = match right {
        Some(visibility) => visibility,
        None => EvalVisibility::Public,
    };
    let merged = if eval_visibility_rank(left) < eval_visibility_rank(right) {
        left
    } else {
        right
    };
    match merged {
        EvalVisibility::Public => None,
        visibility => Some(visibility),
    }
}

/// Returns a comparable visibility rank where smaller means more restrictive.
const fn eval_visibility_rank(visibility: EvalVisibility) -> u8 {
    match visibility {
        EvalVisibility::Private => 1,
        EvalVisibility::Protected => 2,
        EvalVisibility::Public => 3,
    }
}

/// Method signature metadata for a runtime eval interface.
#[derive(Debug, Clone)]
pub struct EvalInterfaceMethod {
    name: String,
    source_location: Option<EvalSourceLocation>,
    attributes: Vec<EvalAttribute>,
    is_static: bool,
    params: Vec<String>,
    parameter_attributes: Vec<Vec<EvalAttribute>>,
    parameter_has_types: Vec<bool>,
    parameter_types: Vec<Option<EvalParameterType>>,
    parameter_defaults: Vec<Option<EvalExpr>>,
    parameter_is_by_ref: Vec<bool>,
    parameter_is_variadic: Vec<bool>,
    return_type: Option<EvalParameterType>,
}

impl PartialEq for EvalInterfaceMethod {
    /// Compares interface method metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.attributes == other.attributes
            && self.is_static == other.is_static
            && self.params == other.params
            && self.parameter_attributes == other.parameter_attributes
            && self.parameter_has_types == other.parameter_has_types
            && self.parameter_types == other.parameter_types
            && self.parameter_defaults == other.parameter_defaults
            && self.parameter_is_by_ref == other.parameter_is_by_ref
            && self.parameter_is_variadic == other.parameter_is_variadic
            && self.return_type == other.return_type
    }
}

impl EvalInterfaceMethod {
    /// Creates one dynamic eval interface method signature.
    pub fn new(name: impl Into<String>, params: Vec<String>) -> Self {
        let parameter_has_types = vec![false; params.len()];
        let parameter_attributes = vec![Vec::new(); params.len()];
        let parameter_types = vec![None; params.len()];
        let parameter_defaults = vec![None; params.len()];
        let parameter_is_by_ref = vec![false; params.len()];
        let parameter_is_variadic = vec![false; params.len()];
        Self {
            name: name.into(),
            source_location: None,
            attributes: Vec::new(),
            is_static: false,
            params,
            parameter_attributes,
            parameter_has_types,
            parameter_types,
            parameter_defaults,
            parameter_is_by_ref,
            parameter_is_variadic,
            return_type: None,
        }
    }

    /// Returns a copy of this interface method with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns a copy of this interface method with its static modifier flag set.
    pub fn with_static(mut self, is_static: bool) -> Self {
        self.is_static = is_static;
        self
    }

    /// Returns a copy of this interface method with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns a copy of this interface method with parameter type-presence flags.
    pub fn with_parameter_type_flags(mut self, parameter_has_types: Vec<bool>) -> Self {
        self.parameter_has_types = parameter_has_types;
        self
    }

    /// Returns a copy of this interface method with source-order parameter attributes.
    pub fn with_parameter_attributes(
        mut self,
        parameter_attributes: Vec<Vec<EvalAttribute>>,
    ) -> Self {
        self.parameter_attributes = parameter_attributes;
        self
    }

    /// Returns a copy of this interface method with source-order parameter type metadata.
    pub fn with_parameter_types(mut self, parameter_types: Vec<Option<EvalParameterType>>) -> Self {
        self.parameter_has_types = parameter_types.iter().map(Option::is_some).collect();
        self.parameter_types = parameter_types;
        self
    }

    /// Returns a copy of this interface method with source-order default expressions.
    pub fn with_parameter_defaults(mut self, parameter_defaults: Vec<Option<EvalExpr>>) -> Self {
        self.parameter_defaults = parameter_defaults;
        self
    }

    /// Returns a copy of this interface method with source-order by-reference flags.
    pub fn with_parameter_by_ref_flags(mut self, parameter_is_by_ref: Vec<bool>) -> Self {
        self.parameter_is_by_ref = parameter_is_by_ref;
        self
    }

    /// Returns a copy of this interface method with source-order variadic flags.
    pub fn with_parameter_variadic_flags(mut self, parameter_is_variadic: Vec<bool>) -> Self {
        self.parameter_is_variadic = parameter_is_variadic;
        self
    }

    /// Returns a copy of this interface method with retained return type metadata.
    pub fn with_return_type(mut self, return_type: Option<EvalParameterType>) -> Self {
        self.return_type = return_type;
        self
    }

    /// Returns the PHP-visible method name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns attributes declared directly on this interface method.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns whether this interface method was declared `static`.
    pub const fn is_static(&self) -> bool {
        self.is_static
    }

    /// Returns source-order parameter names without leading `$`.
    pub fn params(&self) -> &[String] {
        &self.params
    }

    /// Returns source-order parameter attributes.
    pub fn parameter_attributes(&self) -> &[Vec<EvalAttribute>] {
        &self.parameter_attributes
    }

    /// Returns source-order flags for whether each parameter declared a type.
    pub fn parameter_has_types(&self) -> &[bool] {
        &self.parameter_has_types
    }

    /// Returns source-order parameter type metadata.
    pub fn parameter_types(&self) -> &[Option<EvalParameterType>] {
        &self.parameter_types
    }

    /// Returns default expressions declared for each source-order parameter.
    pub fn parameter_defaults(&self) -> &[Option<EvalExpr>] {
        &self.parameter_defaults
    }

    /// Returns source-order flags for whether each parameter was declared by reference.
    pub fn parameter_is_by_ref(&self) -> &[bool] {
        &self.parameter_is_by_ref
    }

    /// Returns source-order flags for whether each parameter was declared variadic.
    pub fn parameter_is_variadic(&self) -> &[bool] {
        &self.parameter_is_variadic
    }

    /// Returns retained return type metadata, if the method declared one.
    pub const fn return_type(&self) -> Option<&EvalParameterType> {
        self.return_type.as_ref()
    }
}

/// Runtime class declared by an eval fragment.
#[derive(Debug, Clone)]
pub struct EvalClass {
    name: String,
    source_location: Option<EvalSourceLocation>,
    is_abstract: bool,
    is_final: bool,
    is_readonly_class: bool,
    is_anonymous: bool,
    parent: Option<String>,
    interfaces: Vec<String>,
    attributes: Vec<EvalAttribute>,
    traits: Vec<String>,
    trait_adaptations: Vec<EvalTraitAdaptation>,
    constants: Vec<EvalClassConstant>,
    properties: Vec<EvalClassProperty>,
    methods: Vec<EvalClassMethod>,
}

impl PartialEq for EvalClass {
    /// Compares class metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.is_abstract == other.is_abstract
            && self.is_final == other.is_final
            && self.is_readonly_class == other.is_readonly_class
            && self.is_anonymous == other.is_anonymous
            && self.parent == other.parent
            && self.interfaces == other.interfaces
            && self.attributes == other.attributes
            && self.traits == other.traits
            && self.trait_adaptations == other.trait_adaptations
            && self.constants == other.constants
            && self.properties == other.properties
            && self.methods == other.methods
    }
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
        Self::with_class_modifiers(
            name,
            is_abstract,
            is_final,
            false,
            parent,
            interfaces,
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with class modifiers, optional parent, and interfaces.
    pub fn with_class_modifiers(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        is_readonly_class: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_class_modifiers_and_traits(
            name,
            is_abstract,
            is_final,
            is_readonly_class,
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
        Self::with_class_modifiers_and_traits(
            name,
            is_abstract,
            is_final,
            false,
            parent,
            interfaces,
            traits,
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with class modifiers, relations, and trait uses.
    pub fn with_class_modifiers_and_traits(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        is_readonly_class: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_class_modifiers_traits_and_constants(
            name,
            is_abstract,
            is_final,
            is_readonly_class,
            parent,
            interfaces,
            traits,
            Vec::new(),
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with modifiers, relations, trait uses, constants, and members.
    pub fn with_modifiers_traits_and_constants(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_class_modifiers_traits_and_constants(
            name,
            is_abstract,
            is_final,
            false,
            parent,
            interfaces,
            traits,
            constants,
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with class modifiers, relations, trait uses, constants, and members.
    pub fn with_class_modifiers_traits_and_constants(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        is_readonly_class: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_class_modifiers_traits_adaptations_and_constants(
            name,
            is_abstract,
            is_final,
            is_readonly_class,
            parent,
            interfaces,
            traits,
            Vec::new(),
            constants,
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with modifiers, relations, trait adaptations, constants, and members.
    pub fn with_modifiers_traits_adaptations_and_constants(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        trait_adaptations: Vec<EvalTraitAdaptation>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_class_modifiers_traits_adaptations_and_constants(
            name,
            is_abstract,
            is_final,
            false,
            parent,
            interfaces,
            traits,
            trait_adaptations,
            constants,
            properties,
            methods,
        )
    }

    /// Creates a dynamic eval class with all class modifiers, relations, adaptations, constants, and members.
    pub fn with_class_modifiers_traits_adaptations_and_constants(
        name: impl Into<String>,
        is_abstract: bool,
        is_final: bool,
        is_readonly_class: bool,
        parent: Option<String>,
        interfaces: Vec<String>,
        traits: Vec<String>,
        trait_adaptations: Vec<EvalTraitAdaptation>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self {
            name: name.into(),
            source_location: None,
            is_abstract,
            is_final,
            is_readonly_class,
            is_anonymous: false,
            parent,
            interfaces,
            attributes: Vec::new(),
            traits,
            trait_adaptations,
            constants,
            properties,
            methods,
        }
    }

    /// Returns a copy of this class with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns a copy of this class with optional source-location metadata attached.
    pub const fn with_source_location_option(
        mut self,
        source_location: Option<EvalSourceLocation>,
    ) -> Self {
        self.source_location = source_location;
        self
    }

    /// Returns a copy of this class with class-like attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Marks this eval class metadata as an anonymous class expression result.
    pub fn with_anonymous(mut self) -> Self {
        self.is_anonymous = true;
        self
    }

    /// Marks all properties readonly when this metadata represents a `readonly class`.
    pub fn with_readonly_properties(mut self) -> Self {
        if self.is_readonly_class {
            for property in &mut self.properties {
                property.is_readonly = true;
            }
        }
        self
    }

    /// Returns the original source spelling of this eval-declared class name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns whether this eval-declared class was declared `abstract`.
    pub const fn is_abstract(&self) -> bool {
        self.is_abstract
    }

    /// Returns whether this eval-declared class was declared `final`.
    pub const fn is_final(&self) -> bool {
        self.is_final
    }

    /// Returns whether this eval-declared class was declared `readonly`.
    pub const fn is_readonly_class(&self) -> bool {
        self.is_readonly_class
    }

    /// Returns whether this eval class came from a `new class {}` expression.
    pub const fn is_anonymous(&self) -> bool {
        self.is_anonymous
    }

    /// Returns the parent class name declared by this eval class, when present.
    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    /// Returns interface names implemented directly by this eval class.
    pub fn interfaces(&self) -> &[String] {
        &self.interfaces
    }

    /// Returns attributes declared directly on this eval class.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns trait names used directly by this eval class.
    pub fn traits(&self) -> &[String] {
        &self.traits
    }

    /// Returns trait adaptations declared on this eval class.
    pub fn trait_adaptations(&self) -> &[EvalTraitAdaptation] {
        &self.trait_adaptations
    }

    /// Returns class constants declared directly by this eval class.
    pub fn constants(&self) -> &[EvalClassConstant] {
        &self.constants
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

    /// Returns a class constant by PHP case-sensitive constant name.
    pub fn constant(&self, name: &str) -> Option<&EvalClassConstant> {
        self.constants()
            .iter()
            .find(|constant| constant.name() == name)
    }
}

/// Adaptation rule declared in a runtime eval class `use Trait { ... }` block.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalTraitAdaptation {
    Alias {
        trait_name: Option<String>,
        method: String,
        alias: Option<String>,
        visibility: Option<EvalVisibility>,
    },
    InsteadOf {
        trait_name: Option<String>,
        method: String,
        instead_of: Vec<String>,
    },
}

/// Constant metadata for a runtime eval class.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalClassConstant {
    name: String,
    attributes: Vec<EvalAttribute>,
    visibility: EvalVisibility,
    is_final: bool,
    value: EvalExpr,
}

impl EvalClassConstant {
    /// Creates a public eval class constant with a value expression.
    pub fn new(name: impl Into<String>, value: EvalExpr) -> Self {
        Self::with_visibility(name, EvalVisibility::Public, value)
    }

    /// Creates an eval class constant with explicit PHP visibility.
    pub fn with_visibility(
        name: impl Into<String>,
        visibility: EvalVisibility,
        value: EvalExpr,
    ) -> Self {
        Self::with_visibility_and_final(name, visibility, false, value)
    }

    /// Creates an eval class constant with explicit PHP visibility and finality.
    pub fn with_visibility_and_final(
        name: impl Into<String>,
        visibility: EvalVisibility,
        is_final: bool,
        value: EvalExpr,
    ) -> Self {
        Self {
            name: name.into(),
            attributes: Vec::new(),
            visibility,
            is_final,
            value,
        }
    }

    /// Returns a copy of this class constant with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns the PHP-visible class constant name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns attributes declared directly on this class constant.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns the PHP visibility declared for this constant.
    pub const fn visibility(&self) -> EvalVisibility {
        self.visibility
    }

    /// Returns whether this class constant was declared `final`.
    pub const fn is_final(&self) -> bool {
        self.is_final
    }

    /// Returns the constant initializer expression.
    pub fn value(&self) -> &EvalExpr {
        &self.value
    }
}

/// Runtime trait declared by an eval fragment.
#[derive(Debug, Clone)]
pub struct EvalTrait {
    name: String,
    source_location: Option<EvalSourceLocation>,
    attributes: Vec<EvalAttribute>,
    traits: Vec<String>,
    trait_adaptations: Vec<EvalTraitAdaptation>,
    constants: Vec<EvalClassConstant>,
    properties: Vec<EvalClassProperty>,
    methods: Vec<EvalClassMethod>,
}

impl PartialEq for EvalTrait {
    /// Compares trait metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.attributes == other.attributes
            && self.traits == other.traits
            && self.trait_adaptations == other.trait_adaptations
            && self.constants == other.constants
            && self.properties == other.properties
            && self.methods == other.methods
    }
}

impl EvalTrait {
    /// Creates a dynamic eval trait with public properties and methods.
    pub fn new(
        name: impl Into<String>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_constants(name, Vec::new(), properties, methods)
    }

    /// Creates a dynamic eval trait with constants, properties, and methods.
    pub fn with_constants(
        name: impl Into<String>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
    ) -> Self {
        Self::with_constants_traits_adaptations(
            name,
            constants,
            properties,
            methods,
            Vec::new(),
            Vec::new(),
        )
    }

    /// Creates a dynamic eval trait with trait uses, adaptations, constants, properties, and methods.
    pub fn with_constants_traits_adaptations(
        name: impl Into<String>,
        constants: Vec<EvalClassConstant>,
        properties: Vec<EvalClassProperty>,
        methods: Vec<EvalClassMethod>,
        traits: Vec<String>,
        trait_adaptations: Vec<EvalTraitAdaptation>,
    ) -> Self {
        Self {
            name: name.into(),
            source_location: None,
            attributes: Vec::new(),
            traits,
            trait_adaptations,
            constants,
            properties,
            methods,
        }
    }

    /// Returns a copy of this trait with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns a copy of this trait with class-like attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns the original source spelling of this eval-declared trait name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns attributes declared directly on this eval trait.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns trait names used directly by this eval trait.
    pub fn traits(&self) -> &[String] {
        &self.traits
    }

    /// Returns trait adaptations declared directly by this eval trait.
    pub fn trait_adaptations(&self) -> &[EvalTraitAdaptation] {
        &self.trait_adaptations
    }

    /// Returns constants declared directly by this eval trait.
    pub fn constants(&self) -> &[EvalClassConstant] {
        &self.constants
    }

    /// Returns one trait constant by PHP case-sensitive constant name.
    pub fn constant(&self, name: &str) -> Option<&EvalClassConstant> {
        self.constants()
            .iter()
            .find(|constant| constant.name() == name)
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
    attributes: Vec<EvalAttribute>,
    property_type: Option<EvalParameterType>,
    visibility: EvalVisibility,
    set_visibility: Option<EvalVisibility>,
    is_static: bool,
    is_final: bool,
    is_readonly: bool,
    is_promoted: bool,
    is_abstract: bool,
    has_get_hook: bool,
    has_set_hook: bool,
    requires_get_hook: bool,
    requires_set_hook: bool,
    is_virtual: bool,
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
        Self::with_visibility_static_and_readonly(name, visibility, is_static, false, default)
    }

    /// Creates an eval class property with explicit storage and readonly metadata.
    pub fn with_visibility_static_and_readonly(
        name: impl Into<String>,
        visibility: EvalVisibility,
        is_static: bool,
        is_readonly: bool,
        default: Option<EvalExpr>,
    ) -> Self {
        Self::with_visibility_static_final_and_readonly(
            name,
            visibility,
            is_static,
            false,
            is_readonly,
            default,
        )
    }

    /// Creates an eval class property with explicit storage and modifier metadata.
    pub fn with_visibility_static_final_and_readonly(
        name: impl Into<String>,
        visibility: EvalVisibility,
        is_static: bool,
        is_final: bool,
        is_readonly: bool,
        default: Option<EvalExpr>,
    ) -> Self {
        Self {
            name: name.into(),
            attributes: Vec::new(),
            property_type: None,
            visibility,
            set_visibility: None,
            is_static,
            is_final,
            is_readonly,
            is_promoted: false,
            is_abstract: false,
            has_get_hook: false,
            has_set_hook: false,
            requires_get_hook: false,
            requires_set_hook: false,
            is_virtual: false,
            default,
        }
    }

    /// Returns a copy of this property marked with concrete get/set hook metadata.
    pub const fn with_hooks(mut self, has_get_hook: bool, has_set_hook: bool) -> Self {
        self.has_get_hook = has_get_hook;
        self.has_set_hook = has_set_hook;
        self.is_virtual = has_get_hook || has_set_hook;
        self
    }

    /// Returns a copy of this property with explicit hook virtuality metadata.
    pub const fn with_virtual(mut self, is_virtual: bool) -> Self {
        self.is_virtual = is_virtual;
        self
    }

    /// Returns a copy of this property marked as an abstract hook contract.
    pub const fn with_abstract_hook_contract(
        mut self,
        requires_get_hook: bool,
        requires_set_hook: bool,
    ) -> Self {
        self.is_abstract = true;
        self.requires_get_hook = requires_get_hook;
        self.requires_set_hook = requires_set_hook;
        self.is_virtual = true;
        self
    }

    /// Returns a copy of this property with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns a copy of this property with retained type metadata.
    pub fn with_type(mut self, property_type: Option<EvalParameterType>) -> Self {
        self.property_type = property_type;
        self
    }

    /// Returns a copy of this property with PHP asymmetric write visibility metadata.
    pub const fn with_set_visibility(mut self, set_visibility: Option<EvalVisibility>) -> Self {
        self.set_visibility = set_visibility;
        self
    }

    /// Returns a copy of this property marked as coming from constructor promotion.
    pub const fn with_promoted(mut self) -> Self {
        self.is_promoted = true;
        self
    }

    /// Returns the PHP-visible property name without `$`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns attributes declared directly on this class property.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns retained PHP type metadata for this property.
    pub fn property_type(&self) -> Option<&EvalParameterType> {
        self.property_type.as_ref()
    }

    /// Returns the PHP visibility declared for this property.
    pub const fn visibility(&self) -> EvalVisibility {
        self.visibility
    }

    /// Returns the PHP asymmetric write visibility, if it differs from read visibility.
    pub const fn set_visibility(&self) -> Option<EvalVisibility> {
        self.set_visibility
    }

    /// Returns the visibility that applies to writes for this property.
    pub const fn write_visibility(&self) -> EvalVisibility {
        match self.set_visibility {
            Some(visibility) => visibility,
            None => self.visibility,
        }
    }

    /// Returns whether this property was declared `static`.
    pub const fn is_static(&self) -> bool {
        self.is_static
    }

    /// Returns whether this property was declared `final`.
    pub const fn is_final(&self) -> bool {
        self.is_final
    }

    /// Returns whether this property was declared `readonly`.
    pub const fn is_readonly(&self) -> bool {
        self.is_readonly
    }

    /// Returns whether this property came from constructor property promotion.
    pub const fn is_promoted(&self) -> bool {
        self.is_promoted
    }

    /// Returns whether this property is an abstract property hook contract.
    pub const fn is_abstract(&self) -> bool {
        self.is_abstract
    }

    /// Returns whether this property has a concrete get hook accessor.
    pub const fn has_get_hook(&self) -> bool {
        self.has_get_hook
    }

    /// Returns whether this property has a concrete set hook accessor.
    pub const fn has_set_hook(&self) -> bool {
        self.has_set_hook
    }

    /// Returns whether this abstract property contract requires read access.
    pub const fn requires_get_hook(&self) -> bool {
        self.requires_get_hook
    }

    /// Returns whether this abstract property contract requires write access.
    pub const fn requires_set_hook(&self) -> bool {
        self.requires_set_hook
    }

    /// Returns whether this property is virtual instead of backed by object storage.
    pub const fn is_virtual(&self) -> bool {
        self.is_virtual
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
#[derive(Debug, Clone)]
pub struct EvalClassMethod {
    name: String,
    source_location: Option<EvalSourceLocation>,
    attributes: Vec<EvalAttribute>,
    visibility: EvalVisibility,
    is_static: bool,
    is_abstract: bool,
    is_final: bool,
    params: Vec<String>,
    parameter_attributes: Vec<Vec<EvalAttribute>>,
    parameter_has_types: Vec<bool>,
    parameter_types: Vec<Option<EvalParameterType>>,
    parameter_defaults: Vec<Option<EvalExpr>>,
    parameter_is_by_ref: Vec<bool>,
    parameter_is_variadic: Vec<bool>,
    return_type: Option<EvalParameterType>,
    body: Vec<EvalStmt>,
}

impl PartialEq for EvalClassMethod {
    /// Compares class method metadata while ignoring retained source-location decoration.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.attributes == other.attributes
            && self.visibility == other.visibility
            && self.is_static == other.is_static
            && self.is_abstract == other.is_abstract
            && self.is_final == other.is_final
            && self.params == other.params
            && self.parameter_attributes == other.parameter_attributes
            && self.parameter_has_types == other.parameter_has_types
            && self.parameter_types == other.parameter_types
            && self.parameter_defaults == other.parameter_defaults
            && self.parameter_is_by_ref == other.parameter_is_by_ref
            && self.parameter_is_variadic == other.parameter_is_variadic
            && self.return_type == other.return_type
            && self.body == other.body
    }
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
        let parameter_has_types = vec![false; params.len()];
        let parameter_attributes = vec![Vec::new(); params.len()];
        let parameter_types = vec![None; params.len()];
        let parameter_defaults = vec![None; params.len()];
        let parameter_is_by_ref = vec![false; params.len()];
        let parameter_is_variadic = vec![false; params.len()];
        Self {
            name: name.into(),
            source_location: None,
            attributes: Vec::new(),
            visibility,
            is_static,
            is_abstract,
            is_final,
            params,
            parameter_attributes,
            parameter_has_types,
            parameter_types,
            parameter_defaults,
            parameter_is_by_ref,
            parameter_is_variadic,
            return_type: None,
            body,
        }
    }

    /// Returns a copy of this method with source-location metadata attached.
    pub const fn with_source_location(mut self, source_location: EvalSourceLocation) -> Self {
        self.source_location = Some(source_location);
        self
    }

    /// Returns the PHP-visible method name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns eval-fragment source-location metadata, when retained.
    pub const fn source_location(&self) -> Option<EvalSourceLocation> {
        self.source_location
    }

    /// Returns a copy of this method with declaration attributes attached.
    pub fn with_attributes(mut self, attributes: Vec<EvalAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Returns a copy of this method with source-order parameter type-presence flags.
    pub fn with_parameter_type_flags(mut self, parameter_has_types: Vec<bool>) -> Self {
        self.parameter_has_types = parameter_has_types;
        self
    }

    /// Returns a copy of this method with source-order parameter attributes.
    pub fn with_parameter_attributes(
        mut self,
        parameter_attributes: Vec<Vec<EvalAttribute>>,
    ) -> Self {
        self.parameter_attributes = parameter_attributes;
        self
    }

    /// Returns a copy of this method with source-order parameter type metadata.
    pub fn with_parameter_types(mut self, parameter_types: Vec<Option<EvalParameterType>>) -> Self {
        self.parameter_has_types = parameter_types.iter().map(Option::is_some).collect();
        self.parameter_types = parameter_types;
        self
    }

    /// Returns a copy of this method with source-order default expressions.
    pub fn with_parameter_defaults(mut self, parameter_defaults: Vec<Option<EvalExpr>>) -> Self {
        self.parameter_defaults = parameter_defaults;
        self
    }

    /// Returns a copy of this method with source-order by-reference flags.
    pub fn with_parameter_by_ref_flags(mut self, parameter_is_by_ref: Vec<bool>) -> Self {
        self.parameter_is_by_ref = parameter_is_by_ref;
        self
    }

    /// Returns a copy of this method with source-order variadic flags.
    pub fn with_parameter_variadic_flags(mut self, parameter_is_variadic: Vec<bool>) -> Self {
        self.parameter_is_variadic = parameter_is_variadic;
        self
    }

    /// Returns a copy of this method with retained return type metadata.
    pub fn with_return_type(mut self, return_type: Option<EvalParameterType>) -> Self {
        self.return_type = return_type;
        self
    }

    /// Returns attributes declared directly on this class method.
    pub fn attributes(&self) -> &[EvalAttribute] {
        &self.attributes
    }

    /// Returns a copy of this method with a different PHP-visible name.
    pub fn renamed(&self, name: impl Into<String>) -> Self {
        let mut method = self.clone();
        method.name = name.into();
        method
    }

    /// Returns a copy of this method with a different PHP visibility.
    pub fn with_visibility_override(&self, visibility: EvalVisibility) -> Self {
        let mut method = self.clone();
        method.visibility = visibility;
        method
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

    /// Returns source-order parameter attributes.
    pub fn parameter_attributes(&self) -> &[Vec<EvalAttribute>] {
        &self.parameter_attributes
    }

    /// Returns source-order flags for whether each parameter declared a type.
    pub fn parameter_has_types(&self) -> &[bool] {
        &self.parameter_has_types
    }

    /// Returns source-order parameter type metadata.
    pub fn parameter_types(&self) -> &[Option<EvalParameterType>] {
        &self.parameter_types
    }

    /// Returns default expressions declared for each source-order parameter.
    pub fn parameter_defaults(&self) -> &[Option<EvalExpr>] {
        &self.parameter_defaults
    }

    /// Returns source-order flags for whether each parameter was declared by reference.
    pub fn parameter_is_by_ref(&self) -> &[bool] {
        &self.parameter_is_by_ref
    }

    /// Returns source-order flags for whether each parameter was declared variadic.
    pub fn parameter_is_variadic(&self) -> &[bool] {
        &self.parameter_is_variadic
    }

    /// Returns retained return type metadata, if the method declared one.
    pub const fn return_type(&self) -> Option<&EvalParameterType> {
        self.return_type.as_ref()
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
    Cast {
        target: EvalCastType,
        expr: Box<EvalExpr>,
    },
    Const(EvalConst),
    ConstFetch(String),
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
    Match {
        subject: Box<EvalExpr>,
        arms: Vec<EvalMatchArm>,
        default: Option<Box<EvalExpr>>,
    },
    Clone(Box<EvalExpr>),
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

/// The right-hand side accepted by PHP's `instanceof` operator.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalInstanceOfTarget {
    ClassName(String),
    Expr(Box<EvalExpr>),
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

/// Scalar cast targets supported by runtime eval expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalCastType {
    Int,
    Float,
    String,
    Bool,
}

/// Unary operations supported by the initial EvalIR parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalUnaryOp {
    Plus,
    Negate,
    LogicalNot,
    BitNot,
}
