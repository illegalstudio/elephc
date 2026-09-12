//! Purpose:
//! Defines function signature metadata for user functions, builtins, closures, and callable aliases.
//! Stores parameter names, defaults, variadics, by-reference behavior, and return contracts used by call planning.
//!
//! Called from:
//! - `crate::types::checker::functions`
//! - `crate::types::call_args`
//!
//! Key details:
//! - Builtin signatures must match PHP so named arguments, first-class callables, and mutation semantics stay coherent.

use crate::parser::ast::{AttributeGroup, Expr, ExprKind, TypeExpr};
use crate::span::Span;

use super::PhpType;

#[derive(Debug, Clone, PartialEq)]
/// Metadata for a callable's parameter and return type contract.
///
/// Used by call planning, named-argument resolution, first-class callables,
/// and type inference. Builtin signatures must match PHP for coherence with
/// named arguments, callable aliases, and mutation semantics.
pub struct FunctionSig {
    pub params: Vec<(String, PhpType)>,
    pub param_type_exprs: Vec<Option<TypeExpr>>,
    pub param_attributes: Vec<Vec<AttributeGroup>>,
    pub defaults: Vec<Option<Expr>>,
    pub return_type: PhpType,
    pub declared_return: bool,
    /// `true` when declared with `function &f()` / `fn &()` — the function returns a
    /// reference (alias) to the returned lvalue rather than a copy.
    pub by_ref_return: bool,
    pub ref_params: Vec<bool>,
    pub declared_params: Vec<bool>,
    pub variadic: Option<String>,
    /// `Some(message)` if the declaration carried PHP 8.4 `#[\Deprecated]`.
    /// `Some("")` indicates the attribute was present without an explicit
    /// reason. `None` means the function/method is not deprecated.
    pub deprecation: Option<String>,
}

impl FunctionSig {
    /// Returns whether the CALLEE's frame owns a reference to by-value parameter `index`.
    ///
    /// By-value arrays, hashes, and Mixed cells are rebound to owning shadow slots by
    /// `privatize_container_param`. Mixed parameters own a detached value cell, except resources
    /// whose shared identity receives an independent reference instead.
    ///
    /// The repr matters, not the surface type: `iterable` keeps its own runtime shape (a raw heap
    /// pointer dispatched on the heap-kind tag), so an `iterable` parameter is NOT privatized and
    /// the callee can still hand its argument's payload straight back. The caller must keep its
    /// pass-through alias guard for those, or it frees a value the result still points at.
    ///
    /// Deliberately a pure function of the signature, so the callee (which privatizes) and the
    /// caller (which must then release its owning-temporary argument instead of suppressing it)
    /// can never disagree.
    pub fn param_is_callee_owned(&self, index: usize) -> bool {
        self.params.get(index).is_some_and(|(_, php_type)| {
            Self::parameter_needs_owned_shadow(
                php_type, self.ref_params.get(index).copied().unwrap_or(false),
            )
        })
    }

    /// Returns whether forwarding this parameter by value produces an independent result owner.
    /// Reference parameters keep caller storage, but value returns acquire or clone its payload.
    /// A callable or object returned as Mixed is boxed with its own payload retain, not transferred raw.
    pub(crate) fn returned_parameter_has_independent_owner(&self, index: usize) -> bool {
        self.param_is_callee_owned(index)
            || (!self.by_ref_return
                && self.return_type.codegen_repr() == PhpType::Mixed
                && self.params.get(index).is_some_and(|(_, php_type)| {
                    matches!(
                        php_type.codegen_repr(),
                        PhpType::Callable | PhpType::Object(_)
                    )
                }))
            || (!self.by_ref_return
                && self.ref_params.get(index).copied().unwrap_or(false)
                && self.params.get(index).is_some_and(|(_, php_type)| {
                    crate::ir::Ownership::php_type_needs_lifetime_tracking(php_type)
                }))
    }

    /// Shares the user-call ownership boundary between caller cleanup, lowering, and inlining.
    pub(crate) fn parameter_needs_owned_shadow(php_type: &PhpType, by_ref: bool) -> bool {
        !by_ref && matches!(
            php_type.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
        )
    }
}

/// Upgrades a variadic signature for use as a first-class callable.
///
/// If the variadic parameter is not already typed as `Array`, upgrades it to
/// `Array<Mixed>`. Non-variadic signatures are returned unchanged.
///
/// Called from:
/// - first-class callable lowering in codegen
pub(crate) fn callable_wrapper_sig(sig: &FunctionSig) -> FunctionSig {
    let Some(variadic_name) = sig.variadic.as_ref() else {
        return sig.clone();
    };

    let mut wrapper_sig = sig.clone();
    if let Some((name, ty)) = wrapper_sig.params.last_mut() {
        if name == variadic_name {
            if !matches!(ty, PhpType::Array(_)) {
                *ty = PhpType::Array(Box::new(PhpType::Mixed));
            }
            return wrapper_sig;
        }
    }

    let variadic_index = wrapper_sig.params.len();
    let variadic_type_expr = if wrapper_sig.param_type_exprs.len() > variadic_index {
        wrapper_sig.param_type_exprs.remove(variadic_index)
    } else {
        None
    };
    let variadic_attributes = if wrapper_sig.param_attributes.len() > variadic_index {
        wrapper_sig.param_attributes.remove(variadic_index)
    } else {
        Vec::new()
    };
    let variadic_ref = if wrapper_sig.ref_params.len() > variadic_index {
        wrapper_sig.ref_params.remove(variadic_index)
    } else {
        false
    };
    let variadic_declared = if wrapper_sig.declared_params.len() > variadic_index {
        wrapper_sig.declared_params.remove(variadic_index)
    } else {
        false
    };

    wrapper_sig.params.push((
        variadic_name.clone(),
        PhpType::Array(Box::new(PhpType::Mixed)),
    ));
    wrapper_sig.defaults.push(None);
    wrapper_sig.ref_params.push(variadic_ref);
    wrapper_sig.declared_params.push(variadic_declared);
    wrapper_sig.param_type_exprs.push(variadic_type_expr);
    wrapper_sig.param_attributes.push(variadic_attributes);
    wrapper_sig
}

/// The storage contract a variadic collector needs when a callable DESCRIPTOR may fill it.
///
/// A descriptor container holds exactly the arguments PHP supplied, names included, and the
/// invoker's tail collector copies every unconsumed name into an associative hash. The callee
/// therefore has to read its variadic parameter through the runtime heap kind instead of as
/// static indexed storage, which is exactly what `array<mixed>` means to the backend: a
/// Mixed-element indexed array may carry runtime-promoted hash storage, so its iteration
/// dispatches on the heap kind (`crate::codegen::lower_inst::iterators`) and its release walks
/// hash entries when that is what the block holds (`__rt_decref_array`).
///
/// `array<mixed>` rather than the `iterable` marker a direct unknown-named call installs: that
/// marker has its own runtime shape, and a variadic body is ordinary PHP that may still call
/// `count($rest)` or index it, neither of which accepts `iterable`. Both shapes iterate the same
/// way, and `array<mixed>` is also what `callable_wrapper_sig` already publishes to callers, so
/// the promoted callee and its descriptor agree by construction.
pub(crate) fn descriptor_variadic_container() -> PhpType {
    PhpType::Array(Box::new(PhpType::Mixed))
}

/// The parameter slot `sig`'s variadic collector occupies, when it declares one.
///
/// The collector is the last slot today, including for a `func_get_args()` body: the hidden
/// `__elephc_func_argc` slot `crate::func_args` synthesizes is pushed onto the REGULAR parameter
/// list, and the collector is appended after it. The position is still found by NAME rather than
/// taken as `params.last()`, because "last" is an invariant of a rewrite pass in another module
/// and this contract has to keep holding if that pass ever appends a second hidden slot. Every
/// helper in this contract, and the invoker's own element-type decision, addresses the collector
/// through this index, so none of them can disagree about which slot they are describing.
pub(crate) fn variadic_param_index(sig: &FunctionSig) -> Option<usize> {
    let variadic_name = sig.variadic.as_ref()?;
    sig.params
        .iter()
        .position(|(name, _)| name == variadic_name)
}

/// Returns whether the collector's STORAGE can physically hold a NAMED tail entry.
///
/// The one question the backend asks: may this slot receive a hash block? It is deliberately
/// asked of the storage type alone, never of `declared_params`, because the SOURCE element
/// contract of a promoted `int ...$xs` still reads "declared" and says nothing about what the
/// slot can hold.
///
/// Two spellings qualify, and both dispatch on the runtime heap kind rather than on a static
/// container shape: [`descriptor_variadic_container`] itself, and the `iterable` marker that
/// `crate::codegen_support::callable_dispatch::static_method_runtime_wrapper_sig` installs on a
/// static-method wrapper for exactly this reason. `crate::codegen::runtime_callable_invoker`
/// already resolves both to a `Mixed` element, so a collector spelled either way reads the
/// entries the invoker's tail collector writes.
pub(crate) fn variadic_storage_accepts_named_entries(sig: &FunctionSig) -> bool {
    let Some(index) = variadic_param_index(sig) else {
        return false;
    };
    match &sig.params[index].1 {
        PhpType::Array(elem) => elem.codegen_repr() == PhpType::Mixed,
        PhpType::Iterable => true,
        _ => false,
    }
}

/// The collector's SOURCE element hint, the half of the contract promotion must never consume.
///
/// `int ...$xs` is stored as ONE parameter whose type describes the COLLECTION, so promoting the
/// collection to [`descriptor_variadic_container`] would erase `int` if the element contract were
/// only ever re-derived from that storage. It is not: the declaration's own element type syntax
/// is kept in `param_type_exprs` at the collector's slot (see
/// `crate::types::checker::functions::resolution::signature`, which chains `decl.variadic_type`
/// there), and `declared_params` records that the source actually wrote one. Direct-call
/// validation resolves THAT, so a promoted callee still rejects `f("x")` on `int ...$xs`.
///
/// Returns `None` for an undeclared collector, which has no source contract to preserve.
pub(crate) fn variadic_source_element_type_expr(sig: &FunctionSig) -> Option<&TypeExpr> {
    let index = variadic_param_index(sig)?;
    if !sig.declared_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    sig.param_type_exprs.get(index)?.as_ref()
}

/// Returns whether `sig`'s variadic collector still carries a shape no descriptor can fill.
///
/// A variadic starts out as `array<int>` (the compiler-wide untyped fallback) or as the declared
/// `array<T>` of an `int ...$xs`, and NEITHER can hold the named entry a descriptor invocation
/// may deliver. A body compiled for one of them reads an associative tail's hash header as
/// indexed storage (entry count as the length, the insertion-order head slot as element 0) and
/// releases it as an indexed array, which leaks every persisted string key. Promoting it to
/// [`descriptor_variadic_container`] is what keeps the callee and the invoker on one container
/// contract.
///
/// A DECLARED collector is promoted just like an undeclared one, because PHP lets a named
/// argument reach `int ...$xs` exactly as it reaches `...$xs`; only the STORAGE moves, and
/// [`variadic_source_element_type_expr`] keeps the `int` that direct calls are checked against.
///
/// A by-reference variadic is excluded: its elements are already `Mixed` cells, so its storage is
/// the descriptor container by construction and the guard below would answer `false` anyway. It
/// is spelled out so the exclusion is a decision rather than a coincidence.
pub(crate) fn variadic_needs_descriptor_container(sig: &FunctionSig) -> bool {
    let Some(index) = variadic_param_index(sig) else {
        return false;
    };
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        return false;
    }
    match &sig.params[index].1 {
        PhpType::Array(elem) => elem.codegen_repr() != PhpType::Mixed,
        // The `iterable` marker has its own heap-kind-dispatched runtime shape, and any other
        // spelling is not a collector this contract knows how to move.
        _ => false,
    }
}

/// Moves `sig`'s variadic collector onto the descriptor container, reporting whether it moved.
///
/// The ONE mutation in this contract. `declared_params`, `ref_params`, `param_type_exprs` and
/// `defaults` are left untouched on purpose: they carry the source contract, and only the
/// storage is a transport decision. Idempotent, so a second descriptor for the same callable
/// reports `false` and nothing downstream re-resolves.
pub(crate) fn promote_variadic_to_descriptor_container(sig: &mut FunctionSig) -> bool {
    if !variadic_needs_descriptor_container(sig) {
        return false;
    }
    let Some(index) = variadic_param_index(sig) else {
        return false;
    };
    sig.params[index].1 = descriptor_variadic_container();
    true
}

/// Looks up a builtin function's canonical call signature.
///
/// Consults the builtin registry first, then the explicitly enumerated
/// compiler-resident language constructs. Returns `None` for untracked or
/// user-defined functions.
///
/// Called from:
/// - type checker builtin validation
/// - first-class callable builtin sig construction
/// - optimizer effect modeling for builtins
pub(crate) fn builtin_call_sig(name: &str) -> Option<FunctionSig> {
    crate::builtins::registry::function_sig(name)
        .or_else(|| compiler_resident_builtin_call_sig(name))
}

/// Returns call signatures for compiler-resident language constructs.
fn compiler_resident_builtin_call_sig(name: &str) -> Option<FunctionSig> {
    match name {
        "eval" => Some(fixed(&["code"])),
        "empty" => Some(with_return_type(fixed(&["value"]), PhpType::Bool)),
        "isset" => Some(with_return_type(
            variadic(&["var"], "vars"),
            PhpType::Bool,
        )),
        "unset" => Some(with_return_type(
            variadic(&["var"], "vars"),
            PhpType::Void,
        )),
        "exit" | "die" => Some(with_return_type(
            optional(&["status"], 0, vec![int_lit(0)]),
            PhpType::Void,
        )),
        "buffer_new" => Some(fixed(&["length"])),
        _ => None,
    }
}

/// Sets the result type on a compiler-resident language-construct signature.
fn with_return_type(mut signature: FunctionSig, return_type: PhpType) -> FunctionSig {
    signature.return_type = return_type;
    signature
}

/// Returns the signature used when a builtin is accessed as a first-class callable.
///
/// The registry-derived signature is the single source used by direct and
/// first-class callable planning.
///
/// Called from:
/// - first-class callable lowering for builtin references
pub(crate) fn first_class_callable_builtin_sig(name: &str) -> Option<FunctionSig> {
    crate::builtins::registry::first_class_callable_sig(name)
}

/// Constructs a signature with all parameters required (no defaults).
fn fixed(params: &[&str]) -> FunctionSig {
    make_sig(params, vec![None; params.len()], None)
}

/// Constructs a signature with some trailing parameters optional.
///
/// `required` indicates how many leading params are mandatory; the rest receive
/// defaults from `optional_defaults` (mapped positionally). Defaults are padded
/// with `None` if fewer are provided than total params.
fn optional(params: &[&str], required: usize, optional_defaults: Vec<Expr>) -> FunctionSig {
    let mut defaults = vec![None; required];
    defaults.extend(optional_defaults.into_iter().map(Some));
    while defaults.len() < params.len() {
        defaults.push(None);
    }
    make_sig(params, defaults, None)
}

/// Constructs a variadic signature — trailing param collects excess arguments as an array.
///
/// `regular_params` lists the fixed parameters; `variadic_name` names the trailing
/// variadic parameter. The variadic param starts as an empty `array` default.
fn variadic(regular_params: &[&str], variadic_name: &str) -> FunctionSig {
    let mut params = regular_params.to_vec();
    params.push(variadic_name);
    let mut defaults = vec![None; regular_params.len()];
    defaults.push(Some(Expr::new(ExprKind::ArrayLiteral(Vec::new()), Span::dummy())));
    make_sig(&params, defaults, Some(variadic_name))
}

/// Low-level `FunctionSig` constructor from raw parts.
///
/// Assembles params as `Mixed` types, sets all other fields from arguments,
/// and defaults `deprecation` to `None`.
fn make_sig(params: &[&str], defaults: Vec<Option<Expr>>, variadic: Option<&str>) -> FunctionSig {
    FunctionSig {
        params: params
            .iter()
            .map(|name| ((*name).to_string(), PhpType::Mixed))
            .collect(),
        param_type_exprs: vec![None; params.len()],
        param_attributes: vec![Vec::new(); params.len()],
        defaults,
        return_type: PhpType::Mixed,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![false; params.len()],
        declared_params: vec![false; params.len()],
        variadic: variadic.map(str::to_string),
        deprecation: None,
    }
}

/// Constructs an `i64` literal expression for use in default parameter values.
fn int_lit(value: i64) -> Expr {
    Expr::new(ExprKind::IntLiteral(value), Span::dummy())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mixed callable returns have independent box owners; raw and reference returns can transfer.
    #[test]
    fn boxed_callable_returns_do_not_transfer_the_argument_descriptor() {
        let mut sig = variadic_sig(vec![("callback".to_string(), PhpType::Callable)]);
        sig.variadic = None;
        assert!(sig.returned_parameter_has_independent_owner(0));
        assert!(!sig.returned_parameter_has_independent_owner(1));
        sig.return_type = PhpType::Callable;
        assert!(!sig.returned_parameter_has_independent_owner(0));
        sig.return_type = PhpType::Mixed;
        sig.by_ref_return = true;
        assert!(!sig.returned_parameter_has_independent_owner(0));
    }

    /// Mixed object returns retain the payload in a fresh box; raw and reference returns can transfer.
    #[test]
    fn boxed_object_returns_do_not_transfer_the_argument_payload() {
        let mut sig = variadic_sig(vec![(
            "object".to_string(),
            PhpType::Object("Owner".to_string()),
        )]);
        sig.variadic = None;
        assert!(sig.returned_parameter_has_independent_owner(0));
        assert!(!sig.returned_parameter_has_independent_owner(1));
        sig.return_type = PhpType::Object("Owner".to_string());
        assert!(!sig.returned_parameter_has_independent_owner(0));
        sig.return_type = PhpType::Mixed;
        sig.by_ref_return = true;
        assert!(!sig.returned_parameter_has_independent_owner(0));
    }

    /// Computes the callable signature metadata for variadic.
    fn variadic_sig(params: Vec<(String, PhpType)>) -> FunctionSig {
        FunctionSig {
            defaults: vec![None; params.len()],
            param_type_exprs: vec![None; params.len()],
            param_attributes: vec![Vec::new(); params.len()],
            return_type: PhpType::Mixed,
            declared_return: false,
            by_ref_return: false,
            ref_params: vec![false; params.len()],
            declared_params: vec![false; params.len()],
            params,
            variadic: Some("values".to_string()),
            deprecation: None,
        }
    }

    /// A variadic collector a DESCRIPTOR may fill must be readable through the runtime heap kind.
    ///
    /// The descriptor invoker copies every unconsumed NAME into an associative tail hash, so a
    /// callee compiled for `array<int>` reads that hash's header as indexed storage: the entry
    /// count becomes the length and the insertion-order head slot becomes element 0. The promoted
    /// marker must also be a fixed point, or every further descriptor would re-resolve the body.
    #[test]
    fn an_untyped_variadic_collector_needs_the_descriptor_container() {
        let mut sig = variadic_sig(vec![
            ("head".to_string(), PhpType::Int),
            ("values".to_string(), PhpType::Array(Box::new(PhpType::Int))),
        ]);
        assert!(
            variadic_needs_descriptor_container(&sig),
            "array<int> storage cannot carry a named tail entry",
        );

        sig.params[1].1 = descriptor_variadic_container();
        assert!(
            !variadic_needs_descriptor_container(&sig),
            "the promoted container must be a fixed point",
        );

        sig.params[1].1 = PhpType::Iterable;
        assert!(
            !variadic_needs_descriptor_container(&sig),
            "the iterable marker is already dispatched on the runtime heap kind",
        );

        sig.params[1].1 = PhpType::Array(Box::new(PhpType::Int));
        sig.declared_params[1] = true;
        sig.param_type_exprs[1] = Some(TypeExpr::Int);
        assert!(
            variadic_needs_descriptor_container(&sig),
            "a declared `int ...$values` cannot hold a named tail entry either",
        );
        assert!(
            promote_variadic_to_descriptor_container(&mut sig),
            "the declared collector's STORAGE moves to the descriptor container",
        );
        assert_eq!(
            sig.params[1].1,
            descriptor_variadic_container(),
            "the promoted collector must be hash-capable",
        );
        assert_eq!(
            variadic_source_element_type_expr(&sig),
            Some(&TypeExpr::Int),
            "promotion must not consume the SOURCE element contract direct calls are checked \
             against",
        );
        assert!(
            !promote_variadic_to_descriptor_container(&mut sig),
            "promotion is idempotent",
        );

        sig.params[1].1 = PhpType::Array(Box::new(PhpType::Int));
        sig.declared_params[1] = false;
        sig.param_type_exprs[1] = None;
        sig.ref_params[1] = true;
        assert!(
            !variadic_needs_descriptor_container(&sig),
            "a by-reference variadic is already Mixed",
        );

        sig.ref_params[1] = false;
        sig.variadic = None;
        assert!(
            !variadic_needs_descriptor_container(&sig),
            "a signature without a variadic has no collector to promote",
        );
    }

    /// The STORAGE question and the SOURCE question are answered independently, in both orders.
    ///
    /// This is the whole point of splitting them. A promoted `int ...$values` must answer "yes, a
    /// name fits" to the backend and "yes, elements are ints" to direct-call validation at the
    /// same time: one collector, two contracts, and a helper that consulted `declared_params` to
    /// decide the storage question (or the storage type to decide the element question) would
    /// collapse them into one wrong answer. The `iterable` marker is asserted next to
    /// `array<mixed>` because the invoker resolves both to a Mixed element, so the gate that
    /// admits a named tail has to admit both too.
    #[test]
    fn the_storage_and_source_halves_of_the_contract_stay_independent() {
        let mut sig = variadic_sig(vec![
            ("head".to_string(), PhpType::Int),
            ("values".to_string(), PhpType::Array(Box::new(PhpType::Int))),
        ]);
        sig.declared_params[1] = true;
        sig.param_type_exprs[1] = Some(TypeExpr::Int);

        assert!(
            !variadic_storage_accepts_named_entries(&sig),
            "an indexed `array<int>` collector cannot hold a string key, declared or not",
        );
        assert_eq!(
            variadic_source_element_type_expr(&sig),
            Some(&TypeExpr::Int),
            "the source element contract is readable BEFORE promotion",
        );

        assert!(promote_variadic_to_descriptor_container(&mut sig));
        assert!(
            variadic_storage_accepts_named_entries(&sig),
            "the promoted collector is what admits a named tail entry",
        );
        assert_eq!(
            variadic_source_element_type_expr(&sig),
            Some(&TypeExpr::Int),
            "and the source element contract is still `int` AFTER it",
        );

        sig.params[1].1 = PhpType::Iterable;
        assert!(
            variadic_storage_accepts_named_entries(&sig),
            "the iterable marker is the other heap-kind-dispatched storage the invoker fills",
        );

        sig.params[1].1 = PhpType::Mixed;
        assert!(
            !variadic_storage_accepts_named_entries(&sig),
            "a collector that is not a container at all admits nothing",
        );

        let undeclared = variadic_sig(vec![(
            "values".to_string(),
            descriptor_variadic_container(),
        )]);
        assert!(
            variadic_storage_accepts_named_entries(&undeclared),
            "an UNDECLARED promoted collector admits a named tail entry as well",
        );
        assert_eq!(
            variadic_source_element_type_expr(&undeclared),
            None,
            "and it has no source element contract to preserve",
        );
    }

    /// The collector is addressed by NAME, so a hidden trailing slot cannot shift the answer.
    ///
    /// `crate::func_args` synthesizes a hidden `__elephc_func_argc` parameter for a body that
    /// calls `func_get_args()`, and it lands in the REGULAR parameter list, ahead of the
    /// collector. The contract does not depend on that: every helper here, and the invoker's
    /// element-type decision, finds the collector through `variadic_param_index`, which is what
    /// this pins. Asserting the index rather than the type is deliberate, because a `params.last()`
    /// rule passes a type assertion by accident whenever the hidden slot happens to sit first.
    #[test]
    fn the_collector_is_addressed_by_name_past_a_hidden_parameter() {
        let mut sig = variadic_sig(vec![
            ("head".to_string(), PhpType::Int),
            (
                crate::func_args::HIDDEN_ARGC_PARAM.to_string(),
                PhpType::Int,
            ),
            ("values".to_string(), PhpType::Array(Box::new(PhpType::Int))),
        ]);

        assert_eq!(
            variadic_param_index(&sig),
            Some(2),
            "the collector is the slot NAMED by `variadic`, not simply the last one",
        );
        assert!(variadic_needs_descriptor_container(&sig));
        assert!(promote_variadic_to_descriptor_container(&mut sig));
        assert_eq!(
            sig.params[2].1,
            descriptor_variadic_container(),
            "promotion moved the collector, not the hidden count slot",
        );
        assert_eq!(
            sig.params[1].1,
            PhpType::Int,
            "the hidden argc slot keeps its Int storage",
        );

        sig.variadic = None;
        assert_eq!(
            variadic_param_index(&sig),
            None,
            "a signature with no collector has no slot to address",
        );
    }

    /// Builds the parameter metadata for callable wrapper sig retypes existing non array variadic.
    #[test]
    fn callable_wrapper_sig_retypes_existing_non_array_variadic_param() {
        let sig = variadic_sig(vec![
            ("format".to_string(), PhpType::Str),
            ("values".to_string(), PhpType::Mixed),
        ]);

        let wrapper_sig = callable_wrapper_sig(&sig);

        assert_eq!(wrapper_sig.params.len(), 2);
        assert_eq!(
            wrapper_sig.params[1],
            (
                "values".to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
            )
        );
        assert_eq!(wrapper_sig.defaults.len(), 2);
        assert_eq!(wrapper_sig.ref_params.len(), 2);
        assert_eq!(wrapper_sig.declared_params.len(), 2);
    }

    /// Builds the parameter metadata for callable wrapper sig appends missing variadic.
    #[test]
    fn callable_wrapper_sig_appends_missing_variadic_param() {
        let sig = variadic_sig(vec![("format".to_string(), PhpType::Str)]);

        let wrapper_sig = callable_wrapper_sig(&sig);

        assert_eq!(wrapper_sig.params.len(), 2);
        assert_eq!(
            wrapper_sig.params[1],
            (
                "values".to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
            )
        );
        assert_eq!(wrapper_sig.defaults.len(), 2);
        assert_eq!(wrapper_sig.ref_params.len(), 2);
        assert_eq!(wrapper_sig.declared_params.len(), 2);
    }

    /// Verifies compiler-resident constructs expose their concrete EIR result types.
    #[test]
    fn compiler_resident_constructs_have_precise_return_types() {
        assert_eq!(
            builtin_call_sig("empty").expect("empty signature").return_type,
            PhpType::Bool,
        );
        assert_eq!(
            builtin_call_sig("isset").expect("isset signature").return_type,
            PhpType::Bool,
        );
        assert_eq!(
            builtin_call_sig("unset").expect("unset signature").return_type,
            PhpType::Void,
        );
        assert_eq!(
            builtin_call_sig("exit").expect("exit signature").return_type,
            PhpType::Void,
        );
    }
}
