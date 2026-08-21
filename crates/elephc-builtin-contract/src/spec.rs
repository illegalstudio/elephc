//! Purpose:
//! Defines the backend-neutral PHP surface contract for one builtin.
//!
//! Called from:
//! - Shared catalog declarations.
//! - Compiler and Magician registries when assembling backend-specific views.
//!
//! Key details:
//! - All fields use static, allocation-free data so contracts can be embedded in
//!   inventory submissions and generated runtime metadata.
//! - Compiler/EvalIR hooks and concrete runtime symbols do not belong here.

use crate::BuiltinId;

/// Functional area used for catalog organization and documentation routing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Area {
    /// String manipulation builtins.
    String,
    /// Array and collection builtins.
    Array,
    /// Mathematical and numeric builtins.
    Math,
    /// Filesystem, stream, and general I/O builtins.
    Io,
    /// System, process, environment, JSON, and time builtins.
    System,
    /// Type inspection and conversion builtins.
    Types,
    /// Callable and reflection-adjacent builtins.
    Callables,
    /// SPL builtins.
    Spl,
    /// Pointer and buffer extension builtins.
    Pointers,
}

/// Describes how a PHP-visible catalog entry reaches executable behavior.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuiltinKind {
    /// Ordinary function-like builtin using normal call argument semantics.
    Function,
    /// PHP language construct with lazy or lvalue-specific semantics.
    LanguageConstruct,
    /// Catalog name whose call syntax is lowered through a dedicated AST/EIR node.
    DedicatedSyntax,
    /// Function supplied by an injected elephc-PHP prelude.
    PreludeProvided,
}

/// Backend-neutral PHP type spelling used by builtin signatures.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TypeSpec {
    /// PHP `int`.
    Int,
    /// PHP `float`.
    Float,
    /// PHP `string`.
    Str,
    /// PHP `bool`.
    Bool,
    /// PHP `mixed` or a shape refined by a backend-specific checker.
    Mixed,
    /// PHP `void` in return position.
    Void,
    /// elephc `pointer` — a raw address, not a PHP type.
    ///
    /// Present because a declaration that cannot say what a builtin returns does not become
    /// silent, it becomes WRONG: the `Area::Pointers` builtins whose check hook returns
    /// `PhpType::Pointer` otherwise have to declare `Mixed`, and every consumer that reads the
    /// DECLARED type rather than the checked one — the PHP-type conversion, the generated docs,
    /// and the result-type fallback lowering takes when it cannot identify a call — then gets
    /// `mixed` for an address, and hands codegen a boxed cell where a raw address belongs.
    ///
    /// A `check` hook makes the declared type non-authoritative, not unused.
    Ptr,
    /// PHP `callable`, as the owned runtime descriptor it lowers to.
    ///
    /// Same reason as `Ptr`: `__elephc_normalize_callable` checks as `PhpType::Callable`, whose
    /// representation is a descriptor rather than a boxed cell.
    Callable,
}

/// Static PHP value used as an optional builtin parameter default.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DefaultSpec {
    /// PHP `null`.
    Null,
    /// Literal integer.
    Int(i64),
    /// Literal boolean.
    Bool(bool),
    /// Literal float.
    Float(f64),
    /// UTF-8 string literal.
    Str(&'static str),
    /// `PHP_INT_MAX` target sentinel.
    IntMax,
    /// Empty indexed array.
    EmptyArray,
}

/// Whether one PHP parameter receives a value or caller-addressable storage.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PassingMode {
    /// Ordinary by-value parameter.
    Value,
    /// By-reference parameter that must bind writable caller storage.
    ByReference,
}

/// Neutral metadata for one fixed PHP builtin parameter.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ParamSpec {
    /// PHP-visible named-argument key.
    pub name: &'static str,
    /// Declared PHP parameter type.
    pub ty: TypeSpec,
    /// Optional default value.
    pub default: Option<DefaultSpec>,
    /// Compatibility mirror of `passing` used by current registry consumers.
    pub by_ref: bool,
    /// For a by-reference parameter the builtin only WRITES: the type the
    /// caller's variable holds afterwards. `None` for in-out or by-value
    /// parameters. Keeping the written type beside the slot it is written
    /// into stops the two from drifting apart.
    pub writes: Option<TypeSpec>,
}

/// Backend-neutral callable signature selected for one catalog consumer.
///
/// Most consumers use the canonical contract fields directly. A small number
/// of runtime surfaces retain a deliberate compatibility profile while their
/// implementation differs from the canonical shared signature.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinSignature {
    /// Fixed parameters in PHP source order.
    pub params: &'static [ParamSpec],
    /// PHP-visible variadic parameter name, when present.
    pub variadic: Option<&'static str>,
    /// Explicit required-parameter count for non-trailing default shapes.
    pub required_param_count: Option<usize>,
}

impl BuiltinSignature {
    /// Returns the number of required parameters represented by this profile.
    pub fn required_param_count(self) -> usize {
        self.required_param_count.unwrap_or_else(|| {
            self.params
                .iter()
                .take_while(|param| param.default.is_none())
                .count()
        })
    }
}

impl ParamSpec {
    /// Returns the normalized passing mode for this parameter.
    pub const fn passing(self) -> PassingMode {
        if self.by_ref {
            PassingMode::ByReference
        } else {
            PassingMode::Value
        }
    }
}

/// Optional native/runtime capability needed by a builtin surface.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuiltinRequirement {
    /// Statically linked Rust bridge library.
    Bridge(&'static str),
    /// Managed or generated runtime capability selected independently of bridges.
    RuntimeCapability(&'static str),
    /// Native library linked on every supported platform when required.
    SystemLibrary(&'static str),
    /// Native library linked only by the macOS target.
    MacOsLibrary(&'static str),
}

/// Canonical backend-neutral contract for one PHP builtin catalog entry.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinContract {
    /// Stable identity shared by backend implementation bindings.
    pub id: BuiltinId,
    /// Canonical lowercase PHP name without a leading namespace separator.
    pub name: &'static str,
    /// Functional catalog area.
    pub area: Area,
    /// Function, construct, dedicated syntax, or prelude surface classification.
    pub kind: BuiltinKind,
    /// Fixed parameters in PHP source order.
    pub params: &'static [ParamSpec],
    /// PHP-visible variadic parameter name, when present.
    pub variadic: Option<&'static str>,
    /// For a variadic tail the builtin only WRITES: the type each caller variable
    /// holds afterwards. `None` for a by-value tail, which is nearly all of them.
    ///
    /// `sscanf()` and `fscanf()` are the tail that writes: php's
    /// `sscanf($s, '%d %s', $n, $w)` fills both variables and neither has to exist
    /// beforehand. Saying so here is what stops the checker reading those arguments
    /// and rejecting the manual's own idiom with `Undefined variable`.
    pub variadic_writes: Option<TypeSpec>,
    /// Optional supported minimum-arity override.
    pub min_args: Option<usize>,
    /// Optional supported maximum-arity override.
    pub max_args: Option<usize>,
    /// Optional exact arity diagnostic override.
    pub arity_error: Option<&'static str>,
    /// Declared PHP return type before backend-specific refinement.
    pub returns: TypeSpec,
    /// Whether the PHP function returns caller-addressable storage.
    pub by_ref_return: bool,
    /// Short generated-documentation summary.
    pub summary: &'static str,
    /// PHP examples included in generated documentation.
    pub examples: &'static [&'static str],
    /// PHP manual fragment, when one exists.
    pub php_manual: Option<&'static str>,
    /// Deprecation text, when applicable.
    pub deprecation: Option<&'static str>,
    /// Whether strict-PHP hides this elephc-only surface.
    pub extension: bool,
    /// Whether this contract is compiler/runtime-internal and not PHP-visible.
    pub internal: bool,
    /// Fixed neutral runtime or bridge requirements.
    pub requirements: &'static [BuiltinRequirement],
}

impl BuiltinContract {
    /// Returns the canonical callable signature stored on this contract.
    pub const fn signature(&self) -> BuiltinSignature {
        BuiltinSignature {
            params: self.params,
            variadic: self.variadic,
            required_param_count: None,
        }
    }

    /// Returns the fixed PHP parameter names that can contain callable descriptors.
    pub fn callback_parameter_names(&self) -> &'static [&'static str] {
        crate::callback_parameters::names(self.id)
    }
}
