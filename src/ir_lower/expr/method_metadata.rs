//! Purpose:
//! Method signatures, aliases, and late-static result metadata.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Returns the checked signature for an instance method call when metadata is available.
pub(in crate::ir_lower) fn method_signature(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
) -> Option<FunctionSig> {
    method_signature_for_receiver_type(ctx, &ctx.builder.value_php_type(object), method)
}

/// Returns the checked signature for an instance method call on a receiver of `object_ty`.
///
/// The type-level core of `method_signature`, for callers that know the receiver's type but have
/// no lowered receiver value, such as a closure signature derived before its body is lowered.
pub(super) fn method_signature_for_receiver_type(
    ctx: &LoweringContext<'_, '_>,
    object_ty: &PhpType,
    method: &str,
) -> Option<FunctionSig> {
    let key = php_symbol_key(method);
    if let Some((class_name, _)) = singular_object_class(object_ty) {
        let normalized = class_name.trim_start_matches('\\');
        // A bare `object` parameter is `Object("")`: one object type with NO class, so the lookups
        // below can only miss, and the call then lowered with no signature while codegen still
        // resolved the callee nominally -- `f(object $o) { $o->m(); }` answered `Call to undefined
        // method A::m()` for an `m` the runtime class declares. It is a dynamic receiver like any
        // other, so it takes the dynamic answer.
        if normalized.is_empty() {
            return program_wide_dynamic_method_signature(ctx, &key);
        }
        if ctx.interfaces.contains_key(normalized) {
            if let Some(mut signature) = narrowed_runtime_method_signature(ctx, normalized, &key) {
                // An interface that DECLARES the method is dispatched through its interface
                // table, whose wrapper returns what the interface declares -- a boxed Mixed for
                // an untyped method -- so the implementors' shared return type would describe a
                // value the call never produces. Twig's `ExtensionInterface::getOperators()` is
                // untyped while every extension inherits `AbstractExtension`'s `return [[], []]`:
                // the call site read the Mixed box as a raw array, `count()` answered 4, and
                // `ExtensionSet` refused CoreExtension. The shared signature still supplies the
                // parameters, whose omitted defaults are materialized here.
                if let Some(interface_signature) = ctx
                    .interfaces
                    .get(normalized)
                    .and_then(|interface_info| interface_info.methods.get(&key))
                {
                    // A variadic tail on the TABLE's signature -- the hidden `...$__elephc_func_args`
                    // an implementation's `func_get_args()` adds -- is part of the ABI the table
                    // dispatches with, so the call must pass its array even when the implementors'
                    // shared signature has none. `ContainerInterface::get($id)` through a nullable
                    // Psr receiver passed two operands for three and refused Symfony's
                    // `ArgumentResolver`, `SessionListener` and console `Application`.
                    if interface_signature.variadic.is_some()
                        && signature.variadic.is_none()
                        && interface_signature.params.len() == signature.params.len() + 1
                    {
                        return Some(interface_signature.clone());
                    }
                    signature.return_type = interface_signature.return_type.clone();
                    signature.by_ref_return = interface_signature.by_ref_return;
                }
                return Some(signature);
            }
            // Implementors that disagree leave no shared signature; the table's own one still
            // fixes the ABI when it carries a variadic tail the call has to pack.
            if let Some(interface_signature) = ctx
                .interfaces
                .get(normalized)
                .and_then(|interface_info| interface_info.methods.get(&key))
                .filter(|interface_signature| interface_signature.variadic.is_some())
            {
                return Some(interface_signature.clone());
            }
        }
        return class_method_signature(ctx, normalized, &key)
            .cloned()
            .or_else(|| narrowed_runtime_method_signature(ctx, normalized, &key));
    }
    if dynamic_method_receiver_needs_mixed_fallback(object_ty) {
        if ctx.has_eval_barrier() {
            return None;
        }
        // A union names the classes the receiver may hold, so ask THEM before asking the program.
        if let Some(signature) = union_member_method_signature(ctx, &object_ty, &key) {
            return Some(signature);
        }
        return program_wide_dynamic_method_signature(ctx, &key);
    }
    None
}

/// The signature every compiled class declaring `method_key` agrees on, when it can stand for a
/// receiver that may hold an object of ANY class.
///
/// In a program that loads code at run time the receiver may hold an object of a class the closed
/// world never compiled, and the compiled classes' agreement says nothing about it. Laravel's
/// `Route` is left to the interpreter, so `PendingResourceRegistration::withTrashed(array
/// $methods = [])` was the only compiled `withTrashed`; the route cache's
/// `->withTrashed($attributes['withTrashed'] ?? false)` converted `false` to that `array`
/// parameter and every request answered 500. Without a signature the class-id dispatch applies
/// each compiled candidate's own parameters and the eval bridge receives the arguments as written.
/// A signature with by-reference parameters is kept: the argument has to be passed as a reference
/// before any callee runs, which no later dispatch can arrange.
fn program_wide_dynamic_method_signature(
    ctx: &LoweringContext<'_, '_>,
    method_key: &str,
) -> Option<FunctionSig> {
    let signature = common_dynamic_method_signature(ctx, method_key)?;
    if ctx.program_loads_code_at_run_time()
        && !signature.ref_params.iter().any(|by_ref| *by_ref)
    {
        return None;
    }
    Some(signature)
}

/// Returns the common signature across the object members a UNION receiver actually admits.
///
/// [`common_dynamic_method_signature`] answers the same question over every class in the program,
/// so the answer depends on classes the receiver can never hold. Any class anywhere declaring the
/// same method name with a different signature made it return `None`, the call then lowered with
/// no signature, and its omitted optional arguments were never materialized -- which codegen meets
/// as an arity mismatch and reports as `Call to a member function m() on null`.
///
/// MEASURED: with `class A` and `class B` both declaring `m(?string $n = null, int $f = 7)`,
/// `function f(A|B $o) { return $o->m(); }` answers `A:NULL/7` like php. Adding an unrelated
/// `class Unrelated { public function m(int $onlyOne = 3) {} }` to the same file -- never called,
/// not in the union -- turns it into `Call to a member function m() on null`; removing it restores
/// the correct answer. Symfony's console meets the same rule on `AttributeAutoconfigurationPass`,
/// whose `\ReflectionClass|\ReflectionMethod|\ReflectionParameter|\ReflectionProperty $reflector`
/// loses `getAttributes()`'s two optional parameters.
///
/// Each member contributes its own implementation's signature, or the shared descendant signature
/// when the member is a base that does not declare the method itself -- the same two sources
/// [`method_signature`] consults for a singular receiver. A `Mixed` member means the receiver is
/// not confined to named classes after all, so the program-wide answer is the right one and this
/// returns `None` to fall back to it.
fn union_member_method_signature(
    ctx: &LoweringContext<'_, '_>,
    object_ty: &PhpType,
    method_key: &str,
) -> Option<FunctionSig> {
    let PhpType::Union(members) = object_ty else {
        return None;
    };
    let mut common: Option<FunctionSig> = None;
    for member in members {
        match member {
            PhpType::Object(class_name) => {
                let normalized = class_name.trim_start_matches('\\');
                if normalized.is_empty() {
                    return None;
                }
                // A member that does not declare the method can never be the callee -- PHP
                // fatals on the receiver at runtime if it turns out to hold one -- so it does
                // not constrain the signature. Returning `None` here instead made
                // `ReflectionClass|Subject` lose `getAttributes`'s defaults because `Subject`
                // has no such method.
                let Some(signature) = class_method_signature(ctx, normalized, method_key)
                    .cloned()
                    .or_else(|| narrowed_runtime_method_signature(ctx, normalized, method_key))
                else {
                    continue;
                };
                match &common {
                    Some(existing) if *existing != signature => return None,
                    Some(_) => {}
                    None => common = Some(signature),
                }
            }
            // `null`, `false` and friends can hold no method; PHP fatals on the receiver before
            // the signature matters, so they do not constrain it.
            PhpType::Void | PhpType::Bool | PhpType::Int | PhpType::Float | PhpType::Str => {}
            _ => return None,
        }
    }
    common
}

/// Returns a shared concrete-subtype signature when a nominal base lacks the requested method.
///
/// PHP permits a runtime instance of a descendant to expose a method absent from the static base
/// type. The checker accepts that path through the same closed-world subtype set; preserving one
/// common signature here lets EIR materialize omitted default arguments before codegen performs
/// the class-id dispatch.
fn narrowed_runtime_method_signature(
    ctx: &LoweringContext<'_, '_>,
    receiver_name: &str,
    method_key: &str,
) -> Option<FunctionSig> {
    memoized_signature(&NARROWED_SIGNATURES, ctx, receiver_name, method_key, || {
        scan_narrowed_runtime_method_signature(ctx, receiver_name, method_key)
    })
}

/// Per-program memo of a whole-class-table signature answer, tagged with the class table it was
/// computed from (address and size), keyed by the scope class and the method key.
type SignatureMemo = (
    usize,
    usize,
    crate::fast_hash::FastMap<(String, String), Option<FunctionSig>>,
);

thread_local! {
    /// [`narrowed_runtime_method_signature`] answers, keyed by receiver class.
    static NARROWED_SIGNATURES: std::cell::RefCell<Option<SignatureMemo>> =
        const { std::cell::RefCell::new(None) };
    /// [`common_dynamic_method_signature`] answers, keyed by the lexical class (private methods
    /// resolve against it).
    static COMMON_SIGNATURES: std::cell::RefCell<Option<SignatureMemo>> =
        const { std::cell::RefCell::new(None) };
}

/// Forgets both memos; each program lowering starts afresh (a later program's class table could
/// reuse an address and a size).
pub(crate) fn reset_method_signature_memos() {
    NARROWED_SIGNATURES.with(|cell| *cell.borrow_mut() = None);
    COMMON_SIGNATURES.with(|cell| *cell.borrow_mut() = None);
    super::array_access_types::reset_class_table_facts();
}

/// Returns the memoized answer for `(scope, method_key)`, computing it once per class table.
///
/// Both questions scan every class in the program, and lowering asks them for every call site
/// through a Mixed or interface receiver: a Symfony build spent about half of its lowering phase
/// in those scans. The class table is fixed while a program lowers, so the answer is too.
fn memoized_signature(
    memo: &'static std::thread::LocalKey<std::cell::RefCell<Option<SignatureMemo>>>,
    ctx: &LoweringContext<'_, '_>,
    scope: &str,
    method_key: &str,
    compute: impl FnOnce() -> Option<FunctionSig>,
) -> Option<FunctionSig> {
    let identity = (ctx.classes as *const _ as usize, ctx.classes.len());
    let key = (scope.to_string(), method_key.to_string());
    let cached = memo.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.as_ref().is_none_or(|(address, len, _)| (*address, *len) != identity) {
            *cell = Some((identity.0, identity.1, Default::default()));
        }
        cell.as_ref().and_then(|(_, _, answers)| answers.get(&key).cloned())
    });
    if let Some(answer) = cached {
        return answer;
    }
    let answer = compute();
    #[cfg(test)]
    SIGNATURE_SCANS.with(|scans| scans.set(scans.get() + 1));
    memo.with(|cell| {
        if let Some((_, _, answers)) = cell.borrow_mut().as_mut() {
            answers.insert(key, answer.clone());
        }
    });
    answer
}

#[cfg(test)]
thread_local! {
    /// Class-table scans run by [`memoized_signature`], for the regression test.
    static SIGNATURE_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Returns the scans run so far and the memoized questions, narrowed ones as `(scope, method)`.
#[cfg(test)]
pub(crate) fn memoized_signature_stats() -> (usize, usize, Vec<(String, String)>) {
    let count = |memo: &'static std::thread::LocalKey<std::cell::RefCell<Option<SignatureMemo>>>| {
        memo.with(|cell| cell.borrow().as_ref().map_or(0, |(_, _, answers)| answers.len()))
    };
    let narrowed = NARROWED_SIGNATURES.with(|cell| {
        cell.borrow().as_ref().map_or_else(Vec::new, |(_, _, answers)| answers.keys().cloned().collect())
    });
    (
        SIGNATURE_SCANS.with(std::cell::Cell::get),
        count(&NARROWED_SIGNATURES) + count(&COMMON_SIGNATURES),
        narrowed,
    )
}

/// Scans the class table for [`narrowed_runtime_method_signature`].
fn scan_narrowed_runtime_method_signature(
    ctx: &LoweringContext<'_, '_>,
    receiver_name: &str,
    method_key: &str,
) -> Option<FunctionSig> {
    let receiver_is_interface = ctx.interfaces.contains_key(receiver_name);
    let mut signature = None;
    for (class_name, class_info) in ctx.classes {
        let compatible = if receiver_is_interface {
            class_implements_interface_for_ir(ctx, class_name, receiver_name)
        } else {
            class_extends_class(ctx, class_name, receiver_name)
        };
        if !compatible {
            continue;
        }
        let Some(candidate) = class_info.methods.get(method_key).cloned() else {
            continue;
        };
        match &signature {
            Some(current) if current != &candidate => return None,
            Some(_) => {}
            None => signature = Some(candidate),
        }
    }
    signature
}

/// Promotes the writable destination used by PDOStatement binding methods to a durable Mixed cell.
pub(super) fn promote_pdo_binding_ref_argument(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
    args: &[Expr],
) {
    if !type_may_be_pdo_statement(ctx, &ctx.builder.value_php_type(object)) {
        return;
    }
    let parameter_name = match php_symbol_key(method).as_str() {
        "bindparam" => "variable",
        "bindcolumn" => "var",
        _ => return,
    };
    let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
    let argument = expanded_args
        .iter()
        .enumerate()
        .find_map(|(index, arg)| match &arg.kind {
            ExprKind::NamedArg { name, value } if name == parameter_name => Some(value.as_ref()),
            ExprKind::NamedArg { .. } => None,
            _ if index == 1 => Some(arg),
            _ => None,
        });
    let Some(Expr {
        kind: ExprKind::Variable(name),
        span,
    }) = argument
    else {
        return;
    };
    ctx.promote_local_mixed_ref_cell(name, Some(*span));
}

/// Returns whether a receiver type can dispatch to PDOStatement binding methods.
fn type_may_be_pdo_statement(ctx: &LoweringContext<'_, '_>, ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(class) => class_extends_class(ctx, class, "PDOStatement"),
        PhpType::Union(members) => members
            .iter()
            .any(|member| type_may_be_pdo_statement(ctx, member)),
        _ => false,
    }
}

/// Returns the conservative return-to-argument alias summary for a method dispatch.
///
/// A non-final receiver type includes every closed-world descendant implementation,
/// because runtime dispatch can select an override. Missing or synthetic summaries
/// therefore fall back to `Unknown` rather than enabling unsafe cleanup.
pub(super) fn method_return_arg_alias(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
) -> ReturnArgAlias {
    let object_ty = ctx.builder.value_php_type(object);
    let method_key = php_symbol_key(method);
    let mut summary: Option<ReturnArgAlias> = None;
    if let Some((class_name, _)) = singular_object_class(&object_ty) {
        let base_class = class_name.trim_start_matches('\\');
        let Some(base_info) = ctx.classes.get(base_class) else {
            // An interface has no class entry: any class declaring the method may answer the
            // call, as for a dynamic receiver. A narrowed interface receiver dispatches a method
            // the interface does not declare that way, and an `Unknown` summary kept its owned
            // arguments alive after the call.
            return declaring_classes_return_arg_alias(ctx, &method_key);
        };
        if base_info.is_final || base_info.final_methods.contains(&method_key) {
            return class_method_return_arg_alias(ctx, base_class, &method_key)
                .unwrap_or(ReturnArgAlias::Unknown);
        }
        // The fold below scans the whole class table, and its answer depends only on the
        // base class, the method name and metadata `LoweringContext` borrows as `&'m`.
        let fold_key = crate::types::DispatchFoldKey {
            base_class: Some(base_class.to_string()),
            method_key: method_key.clone(),
        };
        if let Some(cached) = ctx.return_alias_summaries.cached_dispatch_fold(&fold_key) {
            return cached;
        }
        let family = ctx.return_alias_summaries.class_family(ctx.classes, base_class);
        for candidate in family.iter() {
            let Some(alias) = class_method_return_arg_alias(ctx, candidate, &method_key) else {
                continue;
            };
            summary = Some(match summary {
                Some(current) => current.merge(&alias),
                None => alias,
            });
        }
        let folded = summary.unwrap_or(ReturnArgAlias::Unknown);
        ctx.return_alias_summaries
            .store_dispatch_fold(fold_key, folded.clone());
        return folded;
    }
    if dynamic_method_receiver_needs_mixed_fallback(&object_ty) {
        return declaring_classes_return_arg_alias(ctx, &method_key);
    }
    summary.unwrap_or(ReturnArgAlias::Unknown)
}

/// Merges the return-alias summaries of every class that declares `method_key`, for a receiver
/// whose runtime class can be any of them. `Unknown` behind an eval barrier or when none does.
///
/// The fold has no receiver to narrow by, so it runs over EVERY class and its answer depends on
/// nothing but the method name: it is cached per name.
fn declaring_classes_return_arg_alias(ctx: &LoweringContext<'_, '_>, method_key: &str) -> ReturnArgAlias {
    if ctx.has_eval_barrier() {
        return ReturnArgAlias::Unknown;
    }
    let fold_key = crate::types::DispatchFoldKey {
        base_class: None,
        method_key: method_key.to_string(),
    };
    if let Some(cached) = ctx.return_alias_summaries.cached_dispatch_fold(&fold_key) {
        return cached;
    }
    let mut summary: Option<ReturnArgAlias> = None;
    for candidate in ctx.classes.keys() {
        let Some(alias) = class_method_return_arg_alias(ctx, candidate, method_key) else {
            continue;
        };
        summary = Some(match summary {
            Some(current) => current.merge(&alias),
            None => alias,
        });
    }
    let folded = summary.unwrap_or(ReturnArgAlias::Unknown);
    ctx.return_alias_summaries
        .store_dispatch_fold(fold_key, folded.clone());
    folded
}

/// Resolves one concrete class's dispatched implementation and its source summary.
pub(super) fn class_method_return_arg_alias(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method_key: &str,
) -> Option<ReturnArgAlias> {
    class_method_signature(ctx, class_name, method_key)?;
    let class_info = ctx.classes.get(class_name)?;
    let impl_class = class_info
        .method_impl_classes
        .get(method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    Some(
        ctx.return_alias_summaries
            .method(impl_class, method_key)
            .cloned()
            .unwrap_or(ReturnArgAlias::Unknown),
    )
}

/// Returns a class/interface method signature, preferring the implementing class metadata.
pub(super) fn class_method_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    class_name: &str,
    method_key: &str,
) -> Option<&'a FunctionSig> {
    // A private method belongs to the scope that declared it, not to the receiver's class. Without
    // this the signature was missing for such a call, its DEFAULTED parameters were never
    // materialized, and codegen met the result as `2 operands for 4 ABI params`.
    let normalized = crate::types::private_scope::lexical_private_method_scope(
        ctx.classes,
        ctx.current_class.as_deref(),
        class_name,
        method_key,
    );
    let normalized = normalized
        .as_deref()
        .unwrap_or_else(|| class_name.trim_start_matches('\\'));
    if let Some(class_info) = ctx.classes.get(normalized) {
        let impl_class = class_info
            .method_impl_classes
            .get(method_key)
            .map(String::as_str)
            .unwrap_or(normalized);
        return ctx
            .classes
            .get(impl_class)
            .and_then(|impl_info| impl_info.methods.get(method_key))
            .or_else(|| class_info.methods.get(method_key));
    }
    ctx.interfaces
        .get(normalized)
        .and_then(|interface_info| interface_info.methods.get(method_key))
}

/// Returns the checked return type for an instance method call when metadata is available.
pub(super) fn method_call_result_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
    op: Op,
    expr: &Expr,
) -> PhpType {
    method_call_result_type_for_receiver_type(
        ctx,
        &ctx.builder.value_php_type(object),
        method,
        op,
        expr,
    )
}

/// Returns the checked return type for an instance method call on a receiver of `object_ty`.
///
/// The type-level core of `method_call_result_type`: late-static returns are bound to the
/// receiver class and a nullsafe call on a nullable receiver adds `null`. A closure whose body is
/// `return $o->m();` derives its signature through this before the body is lowered, so the two
/// agree on the result the call hands back (issue #1269).
pub(super) fn method_call_result_type_for_receiver_type(
    ctx: &LoweringContext<'_, '_>,
    object_ty: &PhpType,
    method: &str,
    op: Op,
    expr: &Expr,
) -> PhpType {
    let nullable = singular_object_class(object_ty)
        .map(|(_, nullable)| nullable)
        .unwrap_or(false);
    let Some(return_ty) = method_signature_for_receiver_type(ctx, object_ty, method)
        .filter(|signature| {
            // For a GRADUAL receiver the common signature is one class's, chosen only because it
            // shares the method's name. It may normalize arguments, but it decides the result only
            // if it could actually be CALLED this way — the same arity test the checker applies in
            // `mixed_receiver_method_return_type`. Symfony's `PdoAdapter::doFetch` is why:
            // `$stmt->execute()` matched `Console\Command\Command::execute($input, $output): int`,
            // two required parameters and an unrelated class, so `$result` became `int` and
            // `$result->iterateNumeric()` reached the backend as `method call receiver for PHP
            // type Int`.
            !dynamic_method_receiver_needs_mixed_fallback(object_ty)
                || method_call_argument_count(expr)
                    .is_none_or(|count| signature_accepts_argument_count(signature, count))
        })
        .map(|signature| normalize_value_php_type(signature.return_type))
    else {
        if let Some(return_ty) = narrowed_runtime_method_return_type(ctx, object_ty, method) {
            return if op == Op::NullsafeMethodCall && nullable {
                nullable_result_type(return_ty)
            } else {
                return_ty
            };
        }
        // A named receiver lacks the method only when the checker narrowed it to a class that
        // has it (`if ($animal instanceof Dog)`). Codegen dispatches that call on the runtime
        // class, which hands back a boxed result, as for an untyped receiver.
        if dynamic_method_receiver_needs_mixed_fallback(object_ty)
            || singular_object_class(object_ty).is_some()
        {
            return PhpType::Mixed;
        }
        return fallback_expr_type(expr);
    };
    // A common same-named signature can normalize arguments for a gradual receiver, but it does
    // not prove the receiver belongs to that class, so it decides no result AT ALL -- not a
    // nominal object and not a scalar either. The scalar half is not theoretical: `twig/twig`'s
    // `AbstractTokenParser::$parser` is declared with a docblock and no type, and the only
    // `getEnvironment()` this world compiles is `AppVariable`'s `: string`, so
    // `$this->parser->getEnvironment()->getExpressionParsers()` reached the backend as
    // `method call receiver for PHP type Str`. Codegen boxes a Mixed and dispatches it at
    // runtime, which is what PHP does with a receiver whose class is only known then.
    let return_ty = if dynamic_method_receiver_needs_mixed_fallback(object_ty) {
        PhpType::Mixed
    } else {
        return_ty
    };
    let return_ty = if let Some((receiver_name, _)) = singular_object_class(object_ty) {
        // The late-static type narrows the result to the receiver class, but the callee returns in
        // the storage its signature records: an override that narrows `static|false` to `static`
        // adopts the ancestor's boxed storage, and reading that box as a raw object crashed. The
        // signature wins whenever the two disagree on representation.
        let storage_boxed = matches!(return_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_));
        instance_method_late_static_return_for_ir(ctx, receiver_name, &php_symbol_key(method))
            .map(|return_type| late_static_return_type_for_ir(ctx, &return_type, receiver_name))
            .filter(|late| !storage_boxed || late.codegen_repr() == return_ty.codegen_repr())
            .unwrap_or(return_ty)
    } else {
        return_ty
    };
    if op == Op::NullsafeMethodCall && nullable {
        nullable_result_type(return_ty)
    } else {
        return_ty
    }
}

/// Infers the result shared by concrete runtime classes compatible with a narrowed receiver.
///
/// Flow-sensitive `instanceof` validation can accept a method that is absent from the receiver's
/// original nominal class or interface. EIR retains that original receiver type, so recover the
/// concrete return contract from the same closed-world class set used by backend class-id dispatch.
fn narrowed_runtime_method_return_type(
    ctx: &LoweringContext<'_, '_>,
    receiver_type: &PhpType,
    method: &str,
) -> Option<PhpType> {
    let (receiver_name, _) = singular_object_class(receiver_type)?;
    let receiver_name = receiver_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method);
    let receiver_is_interface = ctx.interfaces.contains_key(receiver_name);
    let candidates = ctx
        .classes
        .iter()
        .filter(|(class_name, _)| {
            receiver_name.is_empty()
                || if receiver_is_interface {
                    class_implements_interface_for_ir(ctx, class_name, receiver_name)
                } else {
                    class_extends_class(ctx, class_name, receiver_name)
                }
        })
        .filter_map(|(_, class_info)| class_info.methods.get(&method_key))
        .map(|signature| normalize_value_php_type(signature.return_type.codegen_repr()))
        .collect::<Vec<_>>();
    normalize_union_members(candidates)
}

/// Returns the return type a closure signature records for a directly returned
/// `$receiver->method(...)` whose receiver has type `object_ty`.
///
/// Mirrors `method_call_result_type` wherever the metadata names the method, and answers boxed
/// `Mixed` wherever the call could dispatch somewhere this metadata cannot see: a receiver that
/// is not an object type, or a method its class does not declare (a `__call` dispatch, or a
/// closure later rebound to a class that does). Boxing is always a sound return contract. The
/// syntactic fallback this replaces answered `Int` for every method call, so a closure returning
/// `$o->name()` handed a string back through an int slot and printed 0 (issue #1269).
pub(in crate::ir_lower) fn closure_return_method_call_type(
    ctx: &LoweringContext<'_, '_>,
    object_ty: &PhpType,
    method: &str,
    expr: &Expr,
) -> PhpType {
    if singular_object_class(object_ty).is_none()
        && !dynamic_method_receiver_needs_mixed_fallback(object_ty)
    {
        return PhpType::Mixed;
    }
    if method_signature_for_receiver_type(ctx, object_ty, method).is_none() {
        return PhpType::Mixed;
    }
    method_call_result_type_for_receiver_type(ctx, object_ty, method, Op::MethodCall, expr)
}

/// Returns preserved late-static return syntax for EIR instance dispatch.
pub(super) fn instance_method_late_static_return_for_ir(
    ctx: &LoweringContext<'_, '_>,
    receiver_type: &str,
    method_key: &str,
) -> Option<TypeExpr> {
    let normalized = receiver_type.trim_start_matches('\\');
    if let Some(class_info) = ctx.classes.get(normalized) {
        if let Some(return_type) = class_info.late_static_method_returns.get(method_key) {
            return Some(return_type.clone());
        }
    }
    ctx.interfaces
        .get(normalized)
        .and_then(|interface_info| interface_info.late_static_method_returns.get(method_key))
        .cloned()
}

/// Binds preserved late-static return syntax to an EIR call-site receiver type.
pub(super) fn late_static_return_type_for_ir(
    ctx: &LoweringContext<'_, '_>,
    return_type: &TypeExpr,
    receiver_type: &str,
) -> PhpType {
    let bound = return_type.substitute_relative_class_types(receiver_type, None);
    normalize_value_php_type(ctx.type_expr_to_php_type_for_value(&bound))
}

/// Returns a common method signature for dynamic receivers when every candidate agrees.
pub(super) fn common_dynamic_method_signature(
    ctx: &LoweringContext<'_, '_>,
    method_key: &str,
) -> Option<FunctionSig> {
    let scope = ctx.current_class.as_deref().unwrap_or("");
    memoized_signature(&COMMON_SIGNATURES, ctx, scope, method_key, || {
        scan_common_dynamic_method_signature(ctx, method_key)
    })
}

/// Scans the class table for [`common_dynamic_method_signature`].
fn scan_common_dynamic_method_signature(
    ctx: &LoweringContext<'_, '_>,
    method_key: &str,
) -> Option<FunctionSig> {
    // Comparing borrowed signatures and cloning only the winner: the previous form cloned a
    // whole `FunctionSig` for every class in the module on every call, to throw all but one away.
    let mut common: Option<&FunctionSig> = None;
    for class_name in ctx.classes.keys() {
        let Some(signature) = class_method_signature(ctx, class_name, method_key) else {
            continue;
        };
        match common {
            Some(existing) if existing != signature => return None,
            Some(_) => {}
            None => common = Some(signature),
        }
    }
    common.cloned()
}

/// Returns the callee signature for a call that passes `arg_count` arguments.
///
/// Identical to [`method_signature`] for a receiver with a single compile-time class. For a
/// GRADUAL one the answer is a common same-named signature chosen across every class that declares
/// the method, and one that could not be CALLED this way is not the callee — so it is dropped
/// rather than used to materialize arguments. Symfony's `HtmlErrorRenderer` reached
/// `$request->headers->get('X-Php-Ob-Level', -1)` through a gradual `headers`, matched a `get`
/// with a by-reference parameter beyond the two supplied, and the backend refused with
/// `receiver-register method call with missing non-value parameter`.
pub(super) fn method_signature_for_call(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    method: &str,
    arg_count: usize,
) -> Option<FunctionSig> {
    let signature = method_signature(ctx, object, method).or_else(|| {
        dynamic_method_receiver_needs_mixed_fallback(&ctx.builder.value_php_type(object))
            .then(|| super::method_calls::mixed_receiver_by_ref_signature(ctx, method))
            .flatten()
    })?;
    if !dynamic_method_receiver_needs_mixed_fallback(&ctx.builder.value_php_type(object)) {
        return Some(signature);
    }
    if !signature_accepts_argument_count(&signature, arg_count) {
        return None;
    }
    // An omitted by-reference parameter is NOT a reason to drop the signature: `pad_omitted_by_ref_args`
    // gives it a real place, and dropping it here instead left the call lowering its arguments
    // without a signature while codegen still resolved the callee nominally — two operands for a
    // three-parameter ABI.
    boxed_receiver_source_signature(ctx, object, Some(signature))
}

/// Drops the hidden `func_get_args()` collector from a boxed receiver's shared signature.
///
/// The signature found for a boxed receiver is the one every compiled class declaring the method
/// shares, so its collector is that family's, not the callee's. Packing surplus arguments into it
/// here handed an INTERPRETED receiver one array where php passes the arguments: Laravel's
/// `$this->make($bootstrapper)->bootstrap($this)` reached an eval-loaded bootstrapper with
/// `bootstrap(array)` because the only compiled `bootstrap` was a kernel's parameterless one. The
/// Mixed dispatch packs each compiled candidate's own collector from the plain operands, and the
/// eval bridge receives them as written.
pub(super) fn boxed_receiver_source_signature(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    sig: Option<FunctionSig>,
) -> Option<FunctionSig> {
    let signature = sig?;
    if !crate::func_args::sig_collects_surplus_args(&signature)
        || !dynamic_method_receiver_needs_mixed_fallback(
            &ctx.builder.value_php_type(object),
        )
    {
        return Some(signature);
    }
    Some(
        crate::codegen_support::source_method_adapters::source_visible_signature(&signature)
            .unwrap_or(signature),
    )
}

/// Returns the number of arguments a method-call expression passes, when it is one.
fn method_call_argument_count(expr: &Expr) -> Option<usize> {
    match &expr.kind {
        ExprKind::MethodCall { args, .. } | ExprKind::NullsafeMethodCall { args, .. } => {
            Some(args.len())
        }
        _ => None,
    }
}

/// Returns whether a signature can be CALLED with `arg_count` arguments.
///
/// Mirrors `Checker::signature_accepts_argument_count`: `defaults` gives the first optional
/// position, and a variadic tail takes any number beyond the fixed parameters.
fn signature_accepts_argument_count(signature: &FunctionSig, arg_count: usize) -> bool {
    // The variadic tail is never required: it collects whatever follows the fixed parameters.
    let fixed = if signature.variadic.is_some() {
        signature.params.len().saturating_sub(1)
    } else {
        signature.params.len()
    };
    let required = signature
        .defaults
        .iter()
        .take(fixed)
        .position(|default| default.is_some())
        .unwrap_or(fixed);
    if arg_count < required {
        return false;
    }
    signature.variadic.is_some() || arg_count <= signature.params.len()
}

/// Returns true when an instance-method receiver has no single compile-time class.
pub(super) fn dynamic_method_receiver_needs_mixed_fallback(php_type: &PhpType) -> bool {
    match php_type {
        PhpType::Mixed => true,
        PhpType::Object(name) => name.is_empty(),
        PhpType::Union(members) => members
            .iter()
            .any(|member| matches!(member, PhpType::Mixed | PhpType::Object(_))),
        _ => false,
    }
}
