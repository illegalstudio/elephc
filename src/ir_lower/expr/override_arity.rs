//! Purpose:
//! Routes an ancestor-typed method call to overrides that declare MORE parameters than the
//! ancestor, so their defaulted extra parameters are materialized.
//!
//! Called from:
//! - `crate::ir_lower::expr::method_calls::lower_method_call`.
//!
//! Key details:
//! - A virtual call site pads arguments from the signature of the class it is typed against.
//!   `Container::make()` calls `$container->load($file)` against `Container::load(string $file)`,
//!   and the generated container overrides it as `load($file, $lazyLoad = true)`: the call passed
//!   one argument and the override read `$lazyLoad` from whatever the register still held — a
//!   stack address, which crashed Symfony `--web` workers in some memory layouts.
//! - The closed world knows every class. For each concrete descendant whose method takes more
//!   parameters (all of the extra ones defaulted), the site compares the runtime class id and
//!   re-types the receiver to that class (`Op::Move`), so the call is lowered against the
//!   override's own signature and pads its defaults. Every other class keeps the original call.
//! - A shared vtable slot cannot carry this: sites typed against the subclass pass the full
//!   argument list through the same slot, so an ancestor-arity thunk would drop their arguments.
//! - Classes that inherit one implementation take one arm: the call it lowers is the same for all
//!   of them. Symfony's dumped container has ~450 `getXxxService` classes extending the container,
//!   so `$container->get($id)` was 456 identical arms, 57 000 lines per site. Their ids are mostly
//!   consecutive, which makes the arm's test a few range compares.

use super::*;

/// Lowers the call through a class-id ladder when some override is wider than the site's
/// signature; `None` leaves the ordinary lowering in charge.
pub(super) fn lower_wider_override_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
    site_sig: Option<&FunctionSig>,
    expr: &Expr,
    local_receiver: bool,
) -> Option<LoweredValue> {
    let site_sig = site_sig?;
    if !plain_positional_call_args(args) {
        return None;
    }
    let base = match ctx.builder.value_php_type(object.value) {
        PhpType::Object(name) => name.trim_start_matches('\\').to_string(),
        _ => return None,
    };
    // `Fiber`'s methods are compiler intrinsics lowered by their own path, never through a class
    // ladder: re-typing the receiver and lowering `start()` against the declared signature padded
    // arguments the intrinsic does not take.
    if php_symbol_key(&base) == "fiber" {
        return None;
    }
    // Surplus arguments are not a reason to stay out: php binds them to an override's extra
    // parameters (or its variadic tail), which is exactly what lowering against it does.
    fixed_param_count(site_sig)?;
    let method_key = php_symbol_key(method);
    // A receiver typed by an interface that declares the method is dispatched through the
    // interface table, whose arguments codegen materializes from the INTERFACE's signature --
    // not from the implementors' shared signature this site was lowered against. That is the
    // storage an implementation has to agree with to be called through the table.
    let table_sig = ctx
        .interfaces
        .get(base.as_str())
        .and_then(|interface| interface.methods.get(&method_key))
        .filter(|interface_sig| interface_sig.params.len() == site_sig.params.len());
    let targets = wider_override_classes(ctx, &base, &method_key, site_sig, table_sig);
    if targets.is_empty() {
        return None;
    }
    // The ladder re-uses the receiver in several branches; an owning temporary would need a
    // release decision per branch. It is parked in a synthetic local first, and the ladder runs
    // on the local's load: `$this->factory->getPasswordHasher($user)->verify($hash, $plain,
    // $salt)` reached Symfony's `MigratingPasswordHasher::verify(..., ?string $salt = null)`
    // through the interface's two-parameter signature, and `$salt` received the surplus
    // collector array (the Symfony demo's login crashed on it). The nullable-receiver path
    // already parks its unboxed receiver (see `lower_call_on_unboxed_receiver`); that local's
    // load counts as an "owning temporary" only so a return can move it, and the slot keeps the
    // reference.
    if ctx.value_is_owning_temporary(object) && !local_receiver {
        let object_ty = PhpType::Object(base);
        let receiver_temp = ctx.declare_synthetic_php_local(object_ty.clone());
        // A PHP-local store takes over the owned temporary's reference.
        ctx.store_local(&receiver_temp, object, object_ty.clone(), Some(expr.span));
        let receiver = ctx.load_local(&receiver_temp, Some(expr.span));
        let call =
            lower_wider_override_dispatch(ctx, receiver, method, args, Some(site_sig), expr, true)
                .unwrap_or_else(|| {
                    lower_method_call_with_receiver(ctx, receiver, method, args, Op::MethodCall, expr)
                });
        let slot = ctx.declare_local(&receiver_temp, object_ty);
        ctx.release_stored_local_value(&receiver_temp, slot, Some(expr.span));
        ctx.emit_void(
            Op::UnsetLocal,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            Op::UnsetLocal.default_effects(),
            Some(expr.span),
        );
        return Some(call);
    }

    let result_type = method_call_result_type(ctx, object.value, method, Op::MethodCall, expr);
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    let merge = ctx
        .builder
        .create_named_block("method.override_arity.merge", Vec::new());
    let class_id = ctx.emit_value(
        Op::ObjectClassId,
        vec![object.value],
        None,
        PhpType::Int,
        Op::ObjectClassId.default_effects(),
        Some(expr.span),
    );
    for arm in targets {
        let hit = ctx
            .builder
            .create_named_block("method.override_arity.hit", Vec::new());
        let mut next = None;
        for &(first, last) in &arm.id_ranges {
            next = Some(emit_class_id_range_test(ctx, class_id.value, first, last, hit, expr));
        }
        let next = next.expect("an arm serves at least one class");

        ctx.builder.position_at_end(hit);
        let narrowed = ctx.emit_value(
            Op::Move,
            vec![object.value],
            None,
            PhpType::Object(arm.class_name),
            Op::Move.default_effects(),
            Some(expr.span),
        );
        let call = lower_method_call_with_receiver(ctx, narrowed, method, args, Op::MethodCall, expr);
        store_value_into_temp(ctx, &temp_name, result_type.clone(), call, expr.span);
        branch_to(ctx, merge);

        ctx.builder.position_at_end(next);
    }
    let call = lower_method_call_with_receiver(ctx, object, method, args, Op::MethodCall, expr);
    store_value_into_temp(ctx, &temp_name, result_type, call, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    Some(take_owned_temp(ctx, &temp_name, expr.span))
}

/// Branches to `hit` when `class_id` lies in `first..=last`, and continues in the returned block
/// otherwise.
fn emit_class_id_range_test(
    ctx: &mut LoweringContext<'_, '_>,
    class_id: crate::ir::ValueId,
    first: u64,
    last: u64,
    hit: crate::ir::BlockId,
    expr: &Expr,
) -> crate::ir::BlockId {
    let miss = ctx
        .builder
        .create_named_block("method.override_arity.next", Vec::new());
    let mut compare = |ctx: &mut LoweringContext<'_, '_>, bound: u64, predicate, then_target| {
        let bound = ctx.emit_value(
            Op::ConstI64,
            Vec::new(),
            Some(Immediate::I64(bound as i64)),
            PhpType::Int,
            Op::ConstI64.default_effects(),
            Some(expr.span),
        );
        let matches = ctx.emit_value(
            Op::ICmp,
            vec![class_id, bound.value],
            Some(Immediate::CmpPredicate(predicate)),
            PhpType::Bool,
            Op::ICmp.default_effects(),
            Some(expr.span),
        );
        ctx.builder.terminate(Terminator::CondBr {
            cond: matches.value,
            then_target,
            then_args: Vec::new(),
            else_target: miss,
            else_args: Vec::new(),
        });
    };
    if first == last {
        compare(ctx, first, crate::ir::CmpPredicate::Eq, hit);
    } else {
        let upper = ctx
            .builder
            .create_named_block("method.override_arity.upper", Vec::new());
        compare(ctx, first, crate::ir::CmpPredicate::Sge, upper);
        ctx.builder.position_at_end(upper);
        compare(ctx, last, crate::ir::CmpPredicate::Sle, hit);
    }
    ctx.builder.position_at_end(miss);
    miss
}

/// The object type a NULLABLE receiver must be narrowed to before the call, when the class-id
/// ladder above has overrides to route; `None` leaves the boxed receiver in charge.
///
/// `?ContainerInterface $locator` holds its object in a Mixed cell, and a call on a Mixed
/// receiver goes straight through the interface table: `$locator->get($id)` entered Symfony's
/// `Container::get(string $id, int $invalidBehavior = ...)` with the second parameter read from
/// whatever the register held. Once the null branch is split off, the receiver is the declared
/// object, and unboxing it lets the ladder pad the override's defaults as for a typed receiver.
pub(super) fn nullable_receiver_ladder_type(
    ctx: &LoweringContext<'_, '_>,
    object: LoweredValue,
    method: &str,
    args: &[Expr],
) -> Option<PhpType> {
    if !plain_positional_call_args(args) {
        return None;
    }
    let object_ty = ctx.builder.value_php_type(object.value);
    if !matches!(object_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return None;
    }
    let (class_name, _) = singular_object_class(&object_ty)?;
    let base = class_name.trim_start_matches('\\').to_string();
    if base.is_empty() || php_symbol_key(&base) == "fiber" {
        return None;
    }
    let site_sig = method_signature(ctx, object.value, method)?;
    fixed_param_count(&site_sig)?;
    let method_key = php_symbol_key(method);
    let table_sig = ctx
        .interfaces
        .get(base.as_str())
        .and_then(|interface| interface.methods.get(&method_key))
        .filter(|interface_sig| interface_sig.params.len() == site_sig.params.len());
    if wider_override_classes(ctx, &base, &method_key, &site_sig, table_sig).is_empty() {
        return None;
    }
    Some(PhpType::Object(base))
}

/// Concrete classes at or below `base` whose `method` the site's signature cannot call directly:
/// it declares more than the site's parameters (every extra one defaulted and by value), its
/// variadic tail sits at another position, or it stores one of the site's parameters in a
/// different representation. Each class is matched by its exact id, so the order only needs to be
/// deterministic. Classes that share an implementation and a signature share an arm.
///
/// The variadic case is the hidden `...$__elephc_func_args` tail that `func_get_args()` gives a
/// whole method family: `Node::getAttribute(string $name)` reads `func_get_arg(1)`, so its tail is
/// parameter 1, while an override `getAttribute($name, $default = null)` carries it as parameter 2.
/// A call typed against `Node` packed its surplus into position 1, and the override read the array
/// as `$default` and its own tail from whatever the register held. Lowering the call against the
/// override's signature binds `$default` and packs the rest the way a direct call would.
///
/// The representation case is an INTERFACE contract implemented with a wider declaration:
/// `CacheItemPoolInterface::getItem(string $key)` is implemented by Symfony's
/// `AbstractAdapterTrait::getItem(mixed $key)`. A site typed against the interface materializes a
/// raw string, the implementation's frame expects a boxed cell, and the key arrived as nothing —
/// every `$pool->getItem($key)` in `CacheTrait::doGet()` asked for a null key. A class ancestor
/// is covered at declaration time (`adopt_ancestor_parameter_storage`), but an interface is not
/// an ancestor there, and an explicitly declared `mixed` is not adopted either: its body was
/// checked against `mixed`. Lowering the call against the implementation's own signature boxes
/// the argument the way a direct call would.
fn wider_override_classes(
    ctx: &LoweringContext<'_, '_>,
    base: &str,
    method_key: &str,
    site_sig: &FunctionSig,
    table_sig: Option<&FunctionSig>,
) -> Vec<WiderOverrideArm> {
    let storage_sig = table_sig.unwrap_or(site_sig);
    // The answer depends only on the receiver type, the method and the storage the signature
    // promises, so every call site sharing them shares it: an interface every class implements
    // otherwise re-walked all of its implementors at each of its call sites.
    let key = format!(
        "{}\0{}\0{:?}\0{:?}\0{:?}\0{:?}",
        php_symbol_key(base.trim_start_matches('\\')),
        method_key,
        storage_sig.variadic,
        storage_sig.ref_params,
        storage_sig
            .params
            .iter()
            .map(|(_, ty)| ty.codegen_repr())
            .collect::<Vec<_>>(),
        storage_sig.params.len(),
    );
    let identity = (ctx.classes as *const _ as usize, ctx.classes.len());
    if let Some(found) = WIDER_OVERRIDE_MEMO.with(|cell| {
        cell.borrow()
            .as_ref()
            .filter(|(table, len, _)| (*table, *len) == identity)
            .and_then(|(_, _, memo)| memo.get(&key).cloned())
    }) {
        return found;
    }
    let found = override_arms(ctx, method_key, compute_wider_override_classes(ctx, base, method_key, storage_sig));
    WIDER_OVERRIDE_MEMO.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.as_ref().is_none_or(|(table, len, _)| (*table, *len) != identity) {
            *cell = Some((identity.0, identity.1, Default::default()));
        }
        if let Some((_, _, memo)) = cell.as_mut() {
            memo.insert(key, found.clone());
        }
    });
    found
}

/// One arm of the ladder: the classes one lowered call serves.
#[derive(Clone)]
struct WiderOverrideArm {
    /// The class the receiver is re-typed to. Any class of the arm lowers to the same call.
    class_name: String,
    /// Sorted inclusive class-id ranges holding exactly the arm's classes.
    id_ranges: Vec<(u64, u64)>,
}

/// Groups `classes` by implementation and signature, in the order of each group's first class.
///
/// The signature is part of the key because a subclass carries its own copy of it, and a copy may
/// have drifted from the implementing class's.
fn override_arms(
    ctx: &LoweringContext<'_, '_>,
    method_key: &str,
    classes: Vec<(String, u64)>,
) -> Vec<WiderOverrideArm> {
    let mut slots: crate::fast_hash::FastMap<String, usize> = Default::default();
    let mut groups: Vec<(String, Vec<u64>)> = Vec::new();
    for (class_name, id) in classes {
        let info = ctx.classes.get(class_name.as_str());
        let implementation = info
            .and_then(|info| info.method_impl_classes.get(method_key))
            .map_or(class_name.as_str(), String::as_str);
        let sig = info.and_then(|info| info.methods.get(method_key));
        let key = format!(
            "{}\0{:?}\0{:?}\0{:?}\0{:?}",
            php_symbol_key(implementation),
            sig.map(|sig| sig.params.iter().map(|(_, ty)| ty.codegen_repr()).collect::<Vec<_>>()),
            sig.map(|sig| &sig.ref_params),
            sig.map(|sig| &sig.variadic),
            sig.map(|sig| sig.return_type.codegen_repr()),
        );
        let slot = *slots.entry(key).or_insert_with(|| {
            groups.push((class_name.clone(), Vec::new()));
            groups.len() - 1
        });
        groups[slot].1.push(id);
    }
    groups
        .into_iter()
        .map(|(class_name, mut ids)| {
            ids.sort_unstable();
            let mut id_ranges: Vec<(u64, u64)> = Vec::new();
            for id in ids {
                match id_ranges.last_mut() {
                    Some((_, last)) if *last + 1 == id => *last = id,
                    _ => id_ranges.push((id, id)),
                }
            }
            WiderOverrideArm { class_name, id_ranges }
        })
        .collect()
}

/// Per-program memo of [`wider_override_classes`], tagged with the class table it was built from.
type WiderOverrideMemo = (usize, usize, crate::fast_hash::FastMap<String, Vec<WiderOverrideArm>>);

thread_local! {
    static WIDER_OVERRIDE_MEMO: std::cell::RefCell<Option<WiderOverrideMemo>> =
        const { std::cell::RefCell::new(None) };
}

/// [`wider_override_classes`] without the memo.
fn compute_wider_override_classes(
    ctx: &LoweringContext<'_, '_>,
    base: &str,
    method_key: &str,
    storage_sig: &FunctionSig,
) -> Vec<(String, u64)> {
    let Some(site_fixed) = fixed_param_count(storage_sig) else {
        return Vec::new();
    };
    let site_variadic = storage_sig.variadic.is_some();
    let mut found = Vec::new();
    // This runs for every method call on an object receiver. Only the concrete classes that
    // declare the method AND are `base` (or extend or implement it) can answer, so it walks the
    // smaller of two per-program indexes instead of every class. Walking the declarers alone was
    // still quadratic for `__construct`: every padding thunk calls its own class's constructor,
    // nearly every class declares one, and each candidate re-walked its ancestry -- a third of
    // lowering the Symfony app.
    let declarers = concrete_method_declarers(ctx, method_key);
    let descendants = classes_that_are_or_implement(ctx, base);
    let candidates: Vec<&String> = if descendants.len() < declarers.len() {
        descendants.iter().collect()
    } else {
        declarers
            .iter()
            .filter(|name| descendants.contains(name.as_str()))
            .collect()
    };
    for name in candidates {
        #[cfg(test)]
        WIDER_OVERRIDE_CANDIDATES.with(|count| count.set(count.get() + 1));
        let Some(info) = ctx.classes.get(name.as_str()) else {
            continue;
        };
        if info.is_abstract {
            continue;
        }
        let Some(sig) = info.methods.get(method_key) else {
            continue;
        };
        let Some(own_fixed) = fixed_param_count(sig) else {
            continue;
        };
        let own_variadic = sig.variadic.is_some();
        if own_fixed < site_fixed {
            continue;
        }
        // A by-reference variadic binds each surplus argument's variable; packing values for it
        // here would drop the references, so such an override keeps the ordinary call.
        if own_variadic && sig.ref_params.get(own_fixed).copied().unwrap_or(false) {
            continue;
        }
        let wider = if own_fixed > site_fixed || own_variadic != site_variadic {
            let extra_defaulted = sig
                .defaults
                .get(site_fixed..own_fixed)
                .is_some_and(|defaults| defaults.iter().all(Option::is_some));
            let extra_by_value = sig
                .ref_params
                .iter()
                .take(own_fixed)
                .skip(site_fixed)
                .all(|by_ref| !by_ref);
            extra_defaulted && extra_by_value
        } else {
            false
        };
        // A constructor is not an override and is never entered through an ancestor's slot
        // (`adopt_ancestor_parameter_storage` exempts it for the same reason), so only the
        // arity rule applies to it.
        if !wider
            && (method_key == "__construct"
                || !stores_site_params_differently(sig, storage_sig, site_fixed))
        {
            continue;
        }
        found.push((name.to_string(), info.class_id));
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// Per-program index from a lowercased class or interface name to every class that is it,
/// extends it or implements it, tagged with the class table it was built from.
type DescendantIndex = (
    usize,
    usize,
    crate::fast_hash::FastMap<String, std::rc::Rc<crate::fast_hash::FastSet<String>>>,
);

#[cfg(test)]
thread_local! {
    /// Candidates `wider_override_classes` examined, so a test can pin its cost to the program
    /// size: a return to the all-declarers walk is quadratic in the class count.
    pub(crate) static WIDER_OVERRIDE_CANDIDATES: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

thread_local! {
    static DESCENDANTS: std::cell::RefCell<Option<DescendantIndex>> =
        const { std::cell::RefCell::new(None) };
}

/// The classes that are `target`, extend it or implement it (directly or through a parent), from
/// an index built once per class table.
fn classes_that_are_or_implement(
    ctx: &LoweringContext<'_, '_>,
    target: &str,
) -> std::rc::Rc<crate::fast_hash::FastSet<String>> {
    let identity = (ctx.classes as *const _ as usize, ctx.classes.len());
    DESCENDANTS.with(|cell| {
        let mut cell = cell.borrow_mut();
        let stale = cell
            .as_ref()
            .is_none_or(|(table, len, _)| (*table, *len) != identity);
        if stale {
            let mut index: crate::fast_hash::FastMap<String, crate::fast_hash::FastSet<String>> =
                Default::default();
            for name in ctx.classes.keys() {
                for key in ancestor_keys(ctx, name) {
                    index.entry(key).or_default().insert(name.clone());
                }
            }
            let index = index
                .into_iter()
                .map(|(key, names)| (key, std::rc::Rc::new(names)))
                .collect();
            *cell = Some((identity.0, identity.1, index));
        }
        cell.as_ref()
            .and_then(|(_, _, index)| index.get(&php_symbol_key(target.trim_start_matches('\\'))).cloned())
            .unwrap_or_default()
    })
}

/// Every lowercased name `class_name` answers to: itself, its parents and every interface any of
/// them implements, at any depth. The walk stops at a parent missing from the class table.
fn ancestor_keys(ctx: &LoweringContext<'_, '_>, class_name: &str) -> crate::fast_hash::FastSet<String> {
    let mut keys = crate::fast_hash::FastSet::default();
    let mut current = Some(class_name.trim_start_matches('\\').to_string());
    while let Some(name) = current {
        if !keys.insert(php_symbol_key(&name)) {
            break;
        }
        let Some(info) = ctx.classes.get(name.as_str()) else {
            break;
        };
        let mut pending: Vec<(String, usize)> =
            info.interfaces.iter().map(|interface| (interface.clone(), 0)).collect();
        while let Some((interface, depth)) = pending.pop() {
            let interface = interface.trim_start_matches('\\');
            if !keys.insert(php_symbol_key(interface)) || depth > 64 {
                continue;
            }
            if let Some(info) = ctx.interfaces.get(interface) {
                pending.extend(info.parents.iter().map(|parent| (parent.clone(), depth + 1)));
            }
        }
        current = info.parent.clone();
    }
    keys
}

/// Per-program index from a method key to the concrete (non-abstract) classes declaring it,
/// tagged with the class table it was built from (address and size).
type MethodDeclarerIndex =
    (usize, usize, crate::fast_hash::FastMap<String, std::rc::Rc<Vec<String>>>);

thread_local! {
    static METHOD_DECLARERS: std::cell::RefCell<Option<MethodDeclarerIndex>> =
        const { std::cell::RefCell::new(None) };
}

/// Forgets the index; each program lowering starts from a fresh one (a later program's class
/// table could reuse an address and a size).
pub(crate) fn reset_method_declarer_index() {
    METHOD_DECLARERS.with(|cell| *cell.borrow_mut() = None);
    DESCENDANTS.with(|cell| *cell.borrow_mut() = None);
    WIDER_OVERRIDE_MEMO.with(|cell| *cell.borrow_mut() = None);
    super::method_metadata::reset_method_signature_memos();
}

/// The concrete classes declaring `method_key`, from an index built once per class table.
fn concrete_method_declarers(
    ctx: &LoweringContext<'_, '_>,
    method_key: &str,
) -> std::rc::Rc<Vec<String>> {
    let identity = (ctx.classes as *const _ as usize, ctx.classes.len());
    METHOD_DECLARERS.with(|cell| {
        let mut cell = cell.borrow_mut();
        let stale = cell
            .as_ref()
            .is_none_or(|(table, len, _)| (*table, *len) != identity);
        if stale {
            let mut index: crate::fast_hash::FastMap<String, Vec<String>> = Default::default();
            for (name, info) in ctx.classes.iter() {
                if info.is_abstract {
                    continue;
                }
                for key in info.methods.keys() {
                    index.entry(key.clone()).or_default().push(name.clone());
                }
            }
            let index = index
                .into_iter()
                .map(|(key, names)| (key, std::rc::Rc::new(names)))
                .collect();
            *cell = Some((identity.0, identity.1, index));
        }
        cell.as_ref()
            .and_then(|(_, _, index)| index.get(method_key).cloned())
            .unwrap_or_default()
    })
}

/// Whether `sig` stores one of the site signature's first `fixed` by-value parameters in another
/// representation.
///
/// A by-reference parameter on either side is left alone: a reference cell is passed the same way
/// whatever it holds.
fn stores_site_params_differently(sig: &FunctionSig, site_sig: &FunctionSig, fixed: usize) -> bool {
    site_sig
        .params
        .iter()
        .zip(sig.params.iter())
        .take(fixed)
        .enumerate()
        .any(|(index, ((_, site_ty), (_, own_ty)))| {
            let by_ref = site_sig.ref_params.get(index).copied().unwrap_or(false)
                || sig.ref_params.get(index).copied().unwrap_or(false);
            !by_ref && site_ty.codegen_repr() != own_ty.codegen_repr()
        })
}

/// The number of parameters before a signature's variadic tail (all of them when it has none, or
/// when the signature lists its variadic apart from `params`); `None` for a variadic listed in
/// the middle, a shape no ladder can re-bind.
fn fixed_param_count(sig: &FunctionSig) -> Option<usize> {
    let Some(name) = sig.variadic.as_deref() else {
        return Some(sig.params.len());
    };
    match sig.params.iter().position(|(param, _)| param == name) {
        None => Some(sig.params.len()),
        Some(index) if index + 1 == sig.params.len() => Some(index),
        Some(_) => None,
    }
}

