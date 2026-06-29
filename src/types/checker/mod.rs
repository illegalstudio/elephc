//! Purpose:
//! Defines the checker state and public type-checking surface for the compiler pipeline.
//! Owns cross-phase metadata including environments, declarations, warnings, FFI, classes, and required libraries.
//!
//! Called from:
//! - `crate::types::check()`
//!
//! Key details:
//! - Checker state is populated in ordered phases; later passes assume schemas, builtins, and signatures are complete.

pub(crate) mod builtins;
mod builtin_enums;
mod builtin_interfaces;
mod builtin_iterators;
mod builtin_json;
mod builtin_spl_classes;
mod builtin_spl_exceptions;
/// builtin_stdclass
pub(crate) mod builtin_stdclass;
mod builtin_types;
mod builtin_user_filter;
mod callables;
/// yield_validation
pub(crate) mod yield_validation;
mod driver;
mod extern_decl;
mod functions;
mod inference;
mod method_pass;
mod schema;
mod stmt_check;
mod type_compat;

use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Platform;
use crate::errors::CompileError;
use crate::parser::ast::{
    CallableTarget, Expr, Program, TypeExpr,
};
use crate::types::{
    CheckResult, ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig,
    InterfaceInfo, PackedClassInfo, PhpType, TypeEnv,
};

pub use inference::{infer_expr_type_syntactic, infer_return_type_syntactic};
pub(crate) use inference::closure_body_uses_this;
pub(crate) use builtin_types::InterfaceDeclInfo;
use builtin_types::validate_magic_method_contracts;
use schema::propagate_abstract_return_types;

/// Checker carries program-wide type-checking state including function signatures,
/// class/interface/enum definitions, variable environments, and warnings collected
/// during type checking.
pub(crate) struct Checker {
    /// Target platform for codegen (affects ABI, sizes, and platform checks).
    pub target_platform: Platform,
    /// User-defined function declarations, keyed by canonical name.
    pub fn_decls: HashMap<String, FnDecl>,
    /// Groups of function variant names that share the same logical function
    /// (used for overload resolution and `function_exists()`).
    pub function_variant_groups: HashMap<String, Vec<String>>,
    /// Canonical function signatures indexed by fully-qualified name.
    pub functions: HashMap<String, FunctionSig>,
    /// Top-level constant types indexed by canonical name.
    pub constants: HashMap<String, PhpType>,
    /// Tracks the return type of closures assigned to variables, keyed by variable name.
    pub closure_return_types: HashMap<String, PhpType>,
    /// Tracks known callable signatures for variables holding first-class callables,
    /// keyed by variable name.
    pub callable_sigs: HashMap<String, FunctionSig>,
    /// Tracks source-declared callable parameters in the active function body.
    pub callable_param_names: HashSet<String>,
    /// Tracks callable signatures inferred for user-function callable parameters,
    /// keyed by (function_name, param_name).
    pub callable_param_sigs: HashMap<(String, String), FunctionSig>,
    /// Tracks which undeclared function parameters have already had their type
    /// adopted from a real call site, keyed by (function_name, param_index). The
    /// first such call adopts the actual argument type; later disagreeing calls
    /// widen the parameter to `Mixed` (so e.g. a parameter called with both an int
    /// and a string is `Mixed`, not collapsed to one type).
    pub param_specialization_seen: HashSet<(String, usize)>,
    /// Tracks callable signatures inferred for user-function callable returns.
    pub callable_return_sigs: HashMap<String, FunctionSig>,
    /// Tracks callable element signatures inferred for user-function array returns.
    pub callable_array_return_sigs: HashMap<String, FunctionSig>,
    /// Tracks capture payloads for closures assigned to variables, keyed by variable name.
    /// Each entry is (capture_name, capture_type, is_by_ref).
    pub callable_captures: HashMap<String, Vec<(String, PhpType, bool)>>,
    /// Tracks callable-array targets assigned to variables, keyed by variable name.
    pub callable_array_targets: HashMap<String, CallableTarget>,
    /// Tracks first-class callable targets assigned to variables, keyed by variable name.
    pub first_class_callable_targets: HashMap<String, CallableTarget>,
    /// Interface definitions collected during the first pass, keyed by canonical name.
    pub interfaces: HashMap<String, InterfaceInfo>,
    /// Class definitions collected during the first pass, keyed by canonical name.
    pub classes: HashMap<String, ClassInfo>,
    /// Canonical class names declared in the program, available for forward references
    /// before the full class definitions are available.
    pub declared_classes: HashSet<String>,
    /// Enum definitions collected during the first pass, keyed by canonical name.
    pub enums: HashMap<String, EnumInfo>,
    /// Canonical interface names declared in the program, available for forward references
    /// before the full interface definitions are available.
    pub declared_interfaces: HashSet<String>,
    /// Name of the class currently being type-checked (used for `$this` resolution).
    pub current_class: Option<String>,
    /// Name of the current method being type-checked, when inside a class body.
    pub current_method: Option<String>,
    /// Whether the current method being type-checked is static.
    pub current_method_is_static: bool,
    /// Whether the function/method/closure body currently being checked returns by
    /// reference (`function &f()`). A `return $obj->prop` in such a body promotes the
    /// property to a reference property (see `reference_property_promotions`).
    pub current_by_ref_return: bool,
    /// Nesting depth of closure bodies currently being type-checked. A non-zero
    /// depth means `$this` is allowed even outside a class method: such a
    /// closure can be bound to an object later via `Closure::bind` / `bindTo`.
    pub closure_depth: usize,
    /// Extern function declarations (e.g. `extern "C" { function foo(): void; }`).
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    /// Extern class (C struct) declarations keyed by canonical name.
    pub extern_classes: HashMap<String, ExternClassInfo>,
    /// Packed layout-only records (`packed class`), keyed by canonical name.
    pub packed_classes: HashMap<String, PackedClassInfo>,
    /// Extern global variable declarations, keyed by variable name.
    pub extern_globals: HashMap<String, PhpType>,
    /// Libraries required by `#[link]` attributes on extern blocks, in link order.
    pub required_libraries: Vec<String>,
    /// Best-known top-level variable types visible to `global` statements in the
    /// current file scope.
    pub top_level_env: TypeEnv,
    /// Names that are by-ref parameters in the current function/closure scope.
    pub active_ref_params: HashSet<String>,
    /// Names introduced via `global` declarations in the current local scope.
    pub active_globals: HashSet<String>,
    /// Names introduced via `static` declarations in the current local scope.
    pub active_statics: HashSet<String>,
    /// Names bound as `foreach` loop keys in the current function/closure scope.
    /// A foreach key is a boxed `Mixed` cell at runtime (`Op::IterCurrentKey`)
    /// even when the checker types it as `Int`/`Str` from the source array, so an
    /// `$dst[$k] = $v` write under such a key must defer the indexed-vs-hash
    /// decision to `Op::ArraySetMixedKey` (destination `Array(Mixed)`) instead of
    /// promoting to `AssocArray` like a statically-known string key would. Mirrors
    /// the lowering's `foreach_int_key_locals` lifetime (per function, not popped).
    pub foreach_key_locals: HashSet<String>,
    /// Active break/continue target depth in the current function or closure body.
    pub break_continue_depth: usize,
    /// Stacks of break/continue depths at each enclosing `finally` block boundary,
    /// used to restore correct depth when branching through `finally`.
    pub finally_break_continue_bases: Vec<usize>,
    /// Warnings raised during type checking (e.g. `#[\Deprecated]` call sites).
    /// Merged with AST-only warnings from `collect_warnings` before being returned
    /// in `CheckResult`.
    pub warnings: Vec<crate::errors::CompileWarning>,
    /// `(class, property)` pairs for regular properties that had a reference taken
    /// (`$x = &$obj->prop`, by-reference return of `$obj->prop`). Recorded while
    /// checking bodies and applied to `classes` after checking so every access lowers
    /// through the property's ref-cell. See `apply_reference_property_promotions`.
    pub reference_property_promotions: HashSet<(String, String)>,
}

#[derive(Clone)]
/// FnDecl stores a user-defined function's declaration metadata: parameter names,
/// types, defaults, variadic marker, return type, span, body statements, and
/// attributes (currently only `#[\Deprecated]` is consulted).
pub(crate) struct FnDecl {
    pub params: Vec<String>,
    pub param_types: Vec<Option<TypeExpr>>,
    pub defaults: Vec<Option<Expr>>,
    pub ref_params: Vec<bool>,
    pub variadic: Option<String>,
    /// Declared element type hint on the variadic parameter (`int ...$xs`), if any.
    pub variadic_type: Option<TypeExpr>,
    pub return_type: Option<TypeExpr>,
    /// `true` when declared with `function &f()` — the function returns a reference.
    pub by_ref_return: bool,
    pub span: crate::span::Span,
    pub body: Vec<crate::parser::ast::Stmt>,
    /// Attribute groups attached to the original `function` declaration.
    /// Currently consulted only for `#[\Deprecated]` detection.
    pub attributes: Vec<crate::parser::ast::AttributeGroup>,
}

/// Runs the type checker on `program` for the given `target_platform`, returning
/// a `CheckResult` on success or a `CompileError` on failure. The checker validates
/// types, resolves declarations, infers return types, and collects warnings. Abstract
/// return types are propagated from concrete implementations before returning.
pub fn check_types(program: &Program, target_platform: Platform) -> Result<CheckResult, CompileError> {
    let (mut checker, global_env) = driver::check_types_impl(program, target_platform)?;

    propagate_abstract_return_types(&mut checker);
    apply_reference_property_promotions(&mut checker);
    validate_magic_method_contracts(&checker)?;

    let mut warnings = crate::types::warnings::collect_warnings(program);
    warnings.extend(checker.warnings);

    Ok(CheckResult {
        global_env,
        functions: checker.functions,
        callable_param_sigs: checker.callable_param_sigs,
        callable_return_sigs: checker.callable_return_sigs,
        callable_array_return_sigs: checker.callable_array_return_sigs,
        interfaces: checker.interfaces,
        classes: checker.classes,
        enums: checker.enums,
        packed_classes: checker.packed_classes,
        extern_functions: checker.extern_functions,
        extern_classes: checker.extern_classes,
        extern_globals: checker.extern_globals,
        required_libraries: checker.required_libraries,
        warnings,
    })
}

/// Returns the single object class named by a type, ignoring a nullable arm.
///
/// `Foo` or `Foo|null` yields `Foo`; unions of multiple classes, `Mixed`, or non-object
/// types yield `None` (so reference promotion only applies to a statically known class).
pub(crate) fn single_object_class_name(ty: &PhpType) -> Option<String> {
    match ty {
        PhpType::Object(name) => Some(name.trim_start_matches('\\').to_string()),
        PhpType::Union(members) => {
            let mut found: Option<String> = None;
            for member in members {
                match member {
                    PhpType::Void => {}
                    PhpType::Object(name) => {
                        let name = name.trim_start_matches('\\').to_string();
                        if found.as_ref().is_some_and(|existing| existing != &name) {
                            return None;
                        }
                        found = Some(name);
                    }
                    _ => return None,
                }
            }
            found
        }
        _ => None,
    }
}

/// Applies recorded reference-property promotions to the class table after body checking.
///
/// A regular property that had a reference taken (`$x = &$obj->prop`, or returned by
/// reference) must be treated as a reference property by codegen so every access lowers
/// through its ref-cell. Promotion is applied to the declaring class and every class that
/// inherits the property, keeping the runtime representation consistent across the
/// hierarchy. Constructor-promoted `&$param` properties already are reference properties
/// (borrowed cell) and are left untouched. Object-owned reference cells are also recorded
/// in `owned_reference_properties` so the object allocates and frees them.
fn apply_reference_property_promotions(checker: &mut Checker) {
    let promotions = std::mem::take(&mut checker.reference_property_promotions);
    for (access_class, prop) in promotions {
        let declaring = checker
            .classes
            .get(&access_class)
            .and_then(|info| info.property_declaring_classes.get(&prop).cloned())
            .unwrap_or_else(|| access_class.clone());
        for info in checker.classes.values_mut() {
            if !info.properties.iter().any(|(name, _)| name == &prop) {
                continue;
            }
            let same_decl = info
                .property_declaring_classes
                .get(&prop)
                .is_some_and(|decl| decl == &declaring);
            if !same_decl {
                continue;
            }
            if info.reference_properties.contains(&prop) {
                continue;
            }
            info.reference_properties.insert(prop.clone());
            info.owned_reference_properties.insert(prop.clone());
        }
    }
}
