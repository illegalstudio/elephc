//! Purpose:
//! Defines the checker state and public type-checking surface for the compiler pipeline.
//! Owns cross-phase metadata including environments, declarations, warnings, FFI, classes, and required libraries.
//!
//! Called from:
//! - `crate::types::check()`
//!
//! Key details:
//! - Checker state is populated in ordered phases; later passes assume schemas, builtins, and signatures are complete.

mod builtin_enums;
mod builtin_interfaces;
mod builtin_iterators;
mod builtin_json;
mod builtin_spl_classes;
mod builtin_class_gate;
mod builtin_spl_exceptions;
/// builtin_stdclass
pub(crate) mod builtin_stdclass;
mod builtin_types;
mod builtin_user_filter;
pub(crate) mod builtins;
mod callables;
mod driver;
mod extern_decl;
mod functions;
mod inference;
mod loop_storage;
mod method_pass;
pub(crate) mod null_probe;
mod schema;
mod stmt_check;
mod type_compat;
/// yield_validation
pub(crate) mod yield_validation;

use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Platform;
use crate::errors::CompileError;
use crate::parser::ast::{
    CallableTarget, Expr, Program, TypeExpr,
};
use crate::span::Span;
use crate::types::{
    collect_attribute_args, collect_attribute_names, CheckResult, ClassInfo, EnumInfo,
    ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo, PackedClassInfo, PhpType,
    ThrowAccessInfo, TypeEnv,
};

pub use inference::{infer_expr_type_syntactic, infer_return_type_syntactic};
pub(crate) use loop_storage::loop_carried_storage_types;
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
    /// Functions whose body is currently being checked.
    ///
    /// Recursive calls may use their provisional signature, but must not re-specialize the same
    /// declaration while it is in flight.
    pub resolving_functions: HashSet<String>,
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
    /// Whether the statement currently being checked was written in a file that opened with
    /// `declare(strict_types=1)`.
    ///
    /// PHP scopes the directive to the file containing the *call site*, so this is installed
    /// from `Stmt::strict_types` by `check_stmt` and restored afterwards. It is `false` outside
    /// any statement check, which keeps class/constant/default-value checking on PHP's coercive
    /// rules — the behaviour elephc had before the directive was honoured.
    pub strict_types: bool,
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
    /// Tracks `ReflectionClass` locals whose reflected class is statically known.
    pub reflection_class_targets: HashMap<String, String>,
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
    /// Canonical trait names declared in the program, available for reflection
    /// and class-like metadata probes that accept traits.
    pub declared_traits: HashSet<String>,
    /// Reflection-visible method signatures declared directly on each trait.
    pub declared_trait_methods: HashMap<String, HashMap<String, FunctionSig>>,
    /// Reflection-visible class constant names declared directly on each trait.
    pub declared_trait_constants: HashMap<String, HashSet<String>>,
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
    /// Whether the active local statement stream has crossed an `eval()` call.
    ///
    /// Once set, unknown local reads are treated as dynamic `Mixed` values because
    /// eval fragments can create caller-scope variables at runtime.
    pub eval_barrier_active: bool,
    /// Types recorded for `return <expr>;` statements at the moment each one was checked,
    /// keyed by the statement node's address in the AST plus its span.
    ///
    /// `collect_return_infos` runs *after* a body has been checked and re-infers every return
    /// against the body's FINAL environment. Without this side channel a flow-sensitive fact
    /// that only holds on part of the body — a property narrowing established halfway down —
    /// would be applied to returns that execute before it, silently accepting a nullable
    /// return. The address key is exact because both passes borrow the same immutable AST; the
    /// span is carried alongside so a recycled address can never be mistaken for a hit.
    pub flow_typed_returns: HashMap<usize, (Span, PhpType)>,
    /// Whether the statements being checked belong to the top-level (global) scope rather than a
    /// function, method, or closure body. `with_local_storage_context` clears it for every local
    /// scope, so `null_probe` only records deferred roots for the scope whose environment
    /// actually becomes `CheckResult::global_env`.
    pub null_probe_scope_is_top_level: bool,
    /// Never-declared variables named by a null probe (`isset`/`empty`/`unset`/`??`) at top level,
    /// with the span to blame if the tolerance turns out to be unjustified.
    ///
    /// PHP answers these probes without an `Undefined variable` warning, but EIR lowering can only
    /// represent the storage when the variable stays `null` for the whole scope: main's local
    /// types come from `global_env`, so a name that is *also* assigned somewhere at top level ends
    /// up with that assigned type and its slot is read before any store. `check_top_level_program`
    /// therefore defers the decision to the end of the pass, where `global_env` is authoritative.
    pub pending_null_probe_roots: Vec<(String, Span)>,
    /// Nesting depth of null-probe operand inference (`isset`/`empty`/`unset` arguments and the
    /// left operand of `??`).
    ///
    /// Inside a probe, reaching through a `null` base is legal PHP — `isset($n['k'])` and
    /// `$n->p ?? $d` answer `false`/`$d` for a null `$n` instead of faulting — so index and
    /// property access on `PhpType::Void` yield `Void` rather than a diagnostic. Outside a probe
    /// those accesses keep their errors.
    pub null_probe_depth: usize,
    /// Active break/continue target depth in the current function or closure body.
    pub break_continue_depth: usize,
    /// Stacks of break/continue depths at each enclosing `finally` block boundary,
    /// used to restore correct depth when branching through `finally`.
    pub finally_break_continue_bases: Vec<usize>,
    /// Stable function-like scope key used to disambiguate identical loop spans.
    pub current_loop_storage_scope: String,
    /// Warnings raised during type checking (e.g. `#[\Deprecated]` call sites).
    /// Merged with AST-only warnings from `collect_warnings` before being returned
    /// in `CheckResult`.
    pub warnings: Vec<crate::errors::CompileWarning>,
    /// `(class, property)` pairs for regular properties that had a reference taken
    /// (`$x = &$obj->prop`, by-reference return of `$obj->prop`). Recorded while
    /// checking bodies and applied to `classes` after checking so every access lowers
    /// through the property's ref-cell. See `apply_reference_property_promotions`.
    pub reference_property_promotions: HashSet<(String, String)>,
    /// Statically-decided access violations that must lower to a catchable
    /// `Error` throw instead of a compile-time error, keyed by source span.
    ///
    /// Record through `record_throw_access_site`, which refuses a span that cannot identify
    /// one: lowering looks entries up by the span of the node it is lowering, so a key shared
    /// by many nodes makes all of them throw. See that method for why the preludes make this
    /// reachable rather than theoretical.
    pub throw_access_sites: HashMap<Span, ThrowAccessInfo>,
    /// Authoritative result type of each checked builtin call, keyed by call span.
    /// EIR lowering consumes this instead of reimplementing builtin return inference.
    pub builtin_call_types: HashMap<Span, PhpType>,
    /// Fixed-point storage contracts keyed by function-like scope and loop span.
    pub loop_storage_types: crate::types::LoopStorageTypes,
    /// `(scope, local)` pairs for `string` locals used as a `++`/`--` target.
    ///
    /// PHP's string increment can change the value's type (`"9"++` is `int(10)`), so EIR
    /// lowering must give those locals boxed `Mixed` frame storage from their FIRST store
    /// instead of widening the slot at the increment. Recorded here because the checker
    /// already visits every expression with a typed environment, so no second AST walk is
    /// needed. See `crate::ir_lower::context::LoweringContext::boxed_incdec_storage_type`.
    pub string_incdec_locals: HashSet<(String, String)>,
}

/// Records an access violation that must lower to a catchable `Error` throw.
///
/// Refuses a span that identifies no single node. `throw_access_sites` is read back by lowering
/// as `get(&span)` for the node being lowered, so an entry filed under a span many nodes share
/// turns ALL of them into throws — a `$this->prop = ...` in an unrelated class would start
/// raising "Cannot modify readonly property".
///
/// That is reachable, not theoretical. Every node the `synthetic_class` builders produce carries
/// `Span::dummy()`, the PDO prelude declares `public readonly string $queryString` and writes it,
/// and the write is allowed only by an `internal_pdo_statement_initializer` special case in
/// `check_property_assignment`. The day that case stops matching, the violation would be filed
/// under the shared dummy span rather than reported. Failing the build is the right answer there:
/// a readonly violation inside a compiler-generated prelude is a compiler bug, not user code to
/// diagnose at run time.
///
/// Takes the map rather than the `Checker` so a caller already holding a borrow of another field
/// — `self.classes`, at both current call sites — can still record.
pub(crate) fn record_throw_access_site(
    sites: &mut HashMap<Span, ThrowAccessInfo>,
    span: Span,
    info: ThrowAccessInfo,
) {
    debug_assert!(
        span.line != 0,
        "throw-access site at a span that names no node ({:?}). Lowering would attach this throw \
         to every other node sharing the span.",
        info.kind,
    );
    if span.line == 0 {
        return;
    }
    sites.insert(span, info);
}

#[derive(Clone)]
/// FnDecl stores a user-defined function's declaration metadata: parameter names,
/// types, defaults, variadic marker, return type, span, body statements, and
/// attributes (currently only `#[\Deprecated]` is consulted).
pub(crate) struct FnDecl {
    pub params: Vec<String>,
    pub param_types: Vec<Option<TypeExpr>>,
    /// Attribute groups aligned with the declared parameters plus the variadic parameter, if any.
    pub param_attributes: Vec<Vec<crate::parser::ast::AttributeGroup>>,
    pub defaults: Vec<Option<Expr>>,
    pub ref_params: Vec<bool>,
    pub variadic: Option<String>,
    /// Whether the variadic parameter was declared by reference (`&...$args`).
    pub variadic_by_ref: bool,
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
pub fn check_types(
    program: &Program,
    target_platform: Platform,
) -> Result<CheckResult, CompileError> {
    let (mut checker, global_env) = driver::check_types_impl(program, target_platform)?;

    propagate_abstract_return_types(&mut checker);
    apply_reference_property_promotions(&mut checker);
    validate_magic_method_contracts(&checker)?;

    let mut warnings = crate::types::warnings::collect_warnings(program);
    warnings.extend(checker.warnings);
    let function_attribute_names = checker
        .fn_decls
        .iter()
        .map(|(name, decl)| (name.clone(), collect_attribute_names(&decl.attributes)))
        .collect();
    let function_attribute_args = checker
        .fn_decls
        .iter()
        .map(|(name, decl)| (name.clone(), collect_attribute_args(&decl.attributes)))
        .collect();
    let return_alias_summaries = crate::types::collect_return_alias_summaries(program);
    dedupe_warnings(&mut warnings);

    Ok(CheckResult {
        global_env,
        functions: checker.functions,
        function_attribute_names,
        function_attribute_args,
        callable_param_sigs: checker.callable_param_sigs,
        return_alias_summaries,
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
        throw_access_sites: checker.throw_access_sites,
        builtin_call_types: checker.builtin_call_types,
        loop_storage_types: checker.loop_storage_types,
        string_incdec_locals: checker.string_incdec_locals,
    })
}

/// Removes duplicate warnings emitted by repeated checker stabilization passes.
///
/// The first occurrence is preserved so diagnostic order stays stable; duplicates are keyed by
/// source span and message, matching the user-visible warning identity.
fn dedupe_warnings(warnings: &mut Vec<crate::errors::CompileWarning>) {
    let mut seen = HashSet::new();
    warnings.retain(|warning| {
        seen.insert((
            warning.span.line,
            warning.span.col,
            warning.message.clone(),
        ))
    });
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
            if let Some(slot) = info.visible_property_index(&prop) {
                if let Some(slot_reference) = info.property_reference_slots.get_mut(slot) {
                    *slot_reference = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod throw_access_site_tests {
    use super::record_throw_access_site;
    use crate::span::Span;
    use crate::types::{ThrowAccessInfo, ThrowAccessKind};
    use std::collections::HashMap;

    fn readonly_violation(span: Span) -> ThrowAccessInfo {
        ThrowAccessInfo {
            span,
            kind: ThrowAccessKind::ReadonlyProperty {
                class_name: "PDOStatement".to_string(),
                property: "queryString".to_string(),
            },
        }
    }

    /// A violation the checker CAN locate is recorded, keyed by where it happened.
    #[test]
    fn a_located_violation_is_recorded() {
        let mut sites = HashMap::new();
        let span = Span::new(7, 3);
        record_throw_access_site(&mut sites, span, readonly_violation(span));
        assert_eq!(sites.len(), 1);
        assert!(sites.contains_key(&span));
    }

    /// A violation the checker CANNOT locate never becomes a key.
    ///
    /// Debug builds abort on it instead — a compiler-generated node that violates readonly is a
    /// compiler bug — so this covers the release behaviour, which is to degrade to silence
    /// rather than attach the throw to every other node carrying the same dummy span.
    #[cfg(not(debug_assertions))]
    #[test]
    fn an_unlocatable_violation_never_becomes_a_key() {
        let mut sites = HashMap::new();
        record_throw_access_site(&mut sites, Span::dummy(), readonly_violation(Span::dummy()));
        assert!(sites.is_empty());
    }
}
