//! Purpose:
//! Merges structurally-identical EIR free functions whose only difference is the concrete class
//! name behind an object type, so instantiations like `identity<User>` and `identity<Order>`
//! collapse to one emitted body.
//!
//! Called from:
//! - `crate::pipeline::compile()` after `ir_passes::optimize_module`, with the exported names.
//!
//! Key details:
//! - Sound only because the guard is exact on everything a class name could reach: the two bodies
//!   must have IDENTICAL instructions, immediates, storage types (`IrType`) and control flow. An
//!   object named nominally (`new T()`, `T::class`, a method/property ref, a class-typed store that
//!   the backend lowers with class-id comparisons) shows up as a differing immediate, so the
//!   comparison refuses it. `has_nominal_use` is a second belt: a body carrying any class-identity
//!   immediate is never merged even if its instructions agree.
//! - Only `module.functions` is merged. Methods are referenced by symbol from vtables and callable
//!   tables built in codegen from `class_info`, so merging them needs a symbol-resolution map; that
//!   is a separate step. Free functions are called by name through `data.function_names`, which is
//!   rewritten here in place, so no codegen change is required.
//! - A function whose name is a string callable (present in `data.strings`) or is exported is left
//!   alone: removing it would strand the name lookup or the export symbol.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BasicBlock, Function, FunctionId, FunctionParam, Immediate, Instruction, LocalSlot, Module,
    Value,
};
use crate::types::PhpType;

/// Collapses a PHP type to the shape the emitted code depends on.
///
/// An object's concrete class is a runtime header, not an instruction: pass, return, store and
/// compare of any object emit the same pointer code, so every `Object(_)` becomes one shape. The
/// element types of arrays and the members of unions are shaped recursively so a container of
/// objects also collapses. `Packed` is deliberately NOT collapsed: a packed class is by-value
/// storage, a different layout entirely.
fn shape(ty: &PhpType) -> PhpType {
    match ty {
        PhpType::Object(_) => PhpType::Object(String::new()),
        // `False` and `Never` are return-type markers the backend never consults on their own: it
        // reads them exactly as `codegen_repr()`'s Bool/Void, so collapsing them is the same
        // equivalence the backend already applies.
        PhpType::False => PhpType::Bool,
        PhpType::Never => PhpType::Void,
        PhpType::Array(inner) => PhpType::Array(Box::new(shape(inner))),
        PhpType::AssocArray { key, value } => PhpType::AssocArray {
            key: Box::new(shape(key)),
            value: Box::new(shape(value)),
        },
        PhpType::Union(members) => PhpType::Union(members.iter().map(shape).collect()),
        other => other.clone(),
    }
}

/// Merges structurally-identical monomorphized free functions, returning how many were removed.
///
/// Only functions whose name carries a type-argument list (`identity<User>`) are candidates: an
/// ordinary user function keeps its body even when two are textually identical, so `#[Export]` and
/// any reflection on an ordinary name stay intact.
pub fn merge_shape_identical_functions(module: &mut Module) -> usize {
    let candidates: Vec<usize> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| is_mergeable_candidate(function, module))
        .map(|(index, _)| index)
        .collect();
    if candidates.len() < 2 {
        return 0;
    }
    let mut buckets: HashMap<CheapKey, Vec<usize>> = HashMap::new();
    for index in candidates {
        buckets
            .entry(cheap_key(&module.functions[index]))
            .or_default()
            .push(index);
    }
    let mut merged_away: HashMap<usize, usize> = HashMap::new();
    for indices in buckets.values() {
        let mut survivors: Vec<usize> = Vec::new();
        for &index in indices {
            let survivor = survivors.iter().copied().find(|&candidate| {
                shape_equal(&module.functions[candidate], &module.functions[index])
            });
            match survivor {
                Some(survivor) => {
                    merged_away.insert(index, survivor);
                }
                None => survivors.push(index),
            }
        }
    }
    if merged_away.is_empty() {
        return 0;
    }
    // Redirect every direct-call name reference to the survivor before the duplicate disappears.
    let renames: Vec<(String, String)> = merged_away
        .iter()
        .map(|(&victim, &survivor)| {
            (
                module.functions[victim].name.clone(),
                module.functions[survivor].name.clone(),
            )
        })
        .collect();
    for entry in module.data.function_names.iter_mut() {
        if let Some((_, survivor)) = renames.iter().find(|(victim, _)| victim == entry) {
            *entry = survivor.clone();
        }
    }
    // Drop the victims, remap the surviving ids, and redirect any FunctionRef immediates.
    let removed = merged_away.len();
    let mut new_functions: Vec<Function> = Vec::with_capacity(module.functions.len() - removed);
    let mut old_to_new: Vec<Option<usize>> = vec![None; module.functions.len()];
    for (old_index, function) in module.functions.drain(..).enumerate() {
        if merged_away.contains_key(&old_index) {
            continue;
        }
        old_to_new[old_index] = Some(new_functions.len());
        new_functions.push(function);
    }
    for (new_index, function) in new_functions.iter_mut().enumerate() {
        function.set_id(FunctionId::from_raw(new_index as u32));
    }
    let resolve = |old: usize| -> Option<u32> {
        let target = merged_away.get(&old).copied().unwrap_or(old);
        old_to_new[target].map(|index| index as u32)
    };
    module.functions = new_functions;
    rewrite_function_refs(module, &resolve);
    removed
}

/// Returns whether `function` may be merged away at all.
fn is_mergeable_candidate(function: &Function, module: &Module) -> bool {
    // A monomorphized name is the only thing worth merging: an ordinary name may be `#[Export]`ed
    // or reflected on, and dropping its symbol would break that.
    function.name.contains('<')
        && !function.flags.is_main
        && !function.flags.is_generator
        && !function.flags.is_synthetic
        && !function.flags.is_closure
        && !function.flags.is_fiber_wrapper
        && !function.flags.is_callback_wrapper
        && !function.flags.is_runtime_callable_invoker
        && !module.data.strings.contains(&function.name)
        && !has_nominal_use(function, &module.data, true)
}

/// A cheap pre-filter so full comparisons only run within plausible groups.
type CheapKey = (usize, usize, usize, usize, String, String);

/// Builds the cheap grouping key: table sizes and the storage-level signature.
fn cheap_key(function: &Function) -> CheapKey {
    (
        function.params.len(),
        function.blocks.len(),
        function.instructions.len(),
        function.values.len(),
        format!("{:?}", function.return_type),
        format!("{:?}", shape(&function.return_php_type)),
    )
}

/// Returns whether the function's instructions could resolve a concrete class the shape has
/// erased.
///
/// Two things qualify. First, a class-identity IMMEDIATE (a class/method/property/enum/variant
/// ref, a type name): those embed a concrete class in the emitted code. Second, an OP whose
/// backend lowering resolves a class from the receiver's STATIC type rather than from an
/// immediate — method dispatch, dynamic property access, writes, object construction — because
/// two instantiations would then silently share one class's resolution.
///
/// A plain property READ is deliberately NOT in the second set. `$this->value` on a `T` slot is
/// the design's canonical non-nominal use: the instruction carries only the property NAME, and the
/// slot it resolves through the receiver's static type is template-identical across instantiations
/// (every object slot is one pointer wide), so `get(): T { return $this->value; }` may merge. A
/// property WRITE stays refused: it carries a `PropertyRef` immediate and runs the typed-property
/// guard, whose accepted-class list is the concrete `T`.
fn has_nominal_use(
    function: &Function,
    data: &crate::ir::DataPool,
    method_dispatch_is_nominal: bool,
) -> bool {
    use crate::ir::Op;
    function.instructions.iter().any(|instruction| {
        if matches!(
            instruction.immediate,
            Some(
                Immediate::ClassRef(_)
                    | Immediate::EnumCaseRef { .. }
                    | Immediate::MethodRef { .. }
                    | Immediate::PropertyRef { .. }
                    | Immediate::ReflectionPropertyRef { .. }
                    | Immediate::FieldRef { .. }
                    | Immediate::FunctionVariantRef { .. }
                    | Immediate::TypeName(_)
            )
        ) {
            return true;
        }
        if matches!(
            instruction.op,
            Op::DynamicPropGet | Op::DynamicPropSet | Op::DynamicPropUnset
        ) {
            // A dynamic access on a NON-object receiver dispatches on the runtime class, which is
            // identical across instantiations; on a statically typed object it resolves the name
            // through the class and stays nominal.
            return nominal_dynamic_receiver(function, instruction);
        }
        if instruction.op == Op::PropSet {
            // A store whose source is a concrete (non-`Mixed`) value needs no typed-property guard,
            // so the only class-specific thing is the slot, which is template-identical across
            // instantiations. A store of a `Mixed` value runs the guard, whose accepted-class list
            // is the concrete `T`, so it stays nominal.
            return !prop_set_source_is_concrete(function, instruction);
        }
        if method_dispatch_is_nominal
            && matches!(instruction.op, Op::MethodCall | Op::NullsafeMethodCall)
        {
            return true;
        }
        if instruction.op == Op::StaticMethodCall {
            return static_call_is_nominal(instruction, data);
        }
        matches!(
            instruction.op,
            Op::EvalStaticMethodCall
                | Op::IteratorMethodCall
                | Op::SplRuntimeCall
                | Op::MethodLookup
                | Op::LoadPropRefCell
                | Op::LoadPropRefCellChecked
                | Op::LoadStaticProperty
                | Op::LoadStaticPropertyRefCell
                | Op::StoreStaticProperty
                | Op::StaticPropInitialized
                | Op::LoadReflectionStaticProperty
                | Op::StoreReflectionStaticProperty
                | Op::ReflectionStaticPropertyInitialized
                | Op::InstanceOf
                | Op::InstanceOfDynamic
                | Op::ObjectNew
                | Op::EvalObjectNew
                | Op::DynamicObjectNew
                | Op::DynamicObjectNewMixed
                | Op::DynamicObjectNewWithoutConstructorMixed
                | Op::DynamicPdoCalledClassStatus
                | Op::DynamicPdoStatementConstructorCall
        )
    })
}

/// Whether every operand of a `prop_set` is concrete (no boxed `Mixed`/`Union` source), so the
/// store runs no typed-property guard.
///
/// The source is checked across ALL operands rather than a fixed index: with two operands the
/// object is a non-`Mixed` pointer and only the value can be boxed, so "no operand is `Mixed`"
/// says exactly "the value is concrete" without depending on the operand order.
fn prop_set_source_is_concrete(function: &Function, instruction: &Instruction) -> bool {
    !instruction.operands.is_empty()
        && instruction.operands.iter().all(|operand| {
            function
                .values
                .get(operand.as_raw() as usize)
                .is_some_and(|value| value.php_type.codegen_repr() != PhpType::Mixed)
        })
}

/// Whether a static method call target is nominal.
///
/// `static::m()` is late-bound and `parent::m()` names a per-instantiation parent, so both are
/// nominal. `self::m()` resolves through the lexical class like an instance dispatch, so it is
/// soft and the call fixpoint decides. A target naming a concrete class carries that class in the
/// immediate: identical strings mean the same class, and differing strings refuse the merge on
/// their own, so returning not-nominal is safe either way.
fn static_call_is_nominal(
    instruction: &Instruction,
    data: &crate::ir::DataPool,
) -> bool {
    let target = match instruction.immediate {
        Some(Immediate::Data(id)) | Some(Immediate::ProfiledData { data: id, .. }) => {
            data.strings.get(id.as_raw() as usize).map(String::as_str)
        }
        _ => None,
    };
    let Some(target) = target else {
        return true;
    };
    let Some((receiver, _)) = target.rsplit_once("::") else {
        return true;
    };
    !receiver.eq_ignore_ascii_case("self")
}

/// Whether a dynamic property access resolves through a statically typed object rather than a
/// runtime-typed receiver.
///
/// A `Mixed`/`Union` receiver dispatches on the runtime class, so the emitted code is the same for
/// every instantiation; a static `Object(_)` receiver resolves the name through its class, which
/// is exactly the class the shape erased.
fn nominal_dynamic_receiver(function: &Function, instruction: &Instruction) -> bool {
    instruction
        .operands
        .first()
        .and_then(|value| function.values.get(value.as_raw() as usize))
        .is_some_and(|value| matches!(value.php_type, PhpType::Object(_)))
}

/// Full structural equality under the object shape, with identity fields excluded.
fn shape_equal(a: &Function, b: &Function) -> bool {
    a.flags == b.flags
        && a.return_type == b.return_type
        && shape(&a.return_php_type) == shape(&b.return_php_type)
        && a.no_epilogue_cleanup_slots == b.no_epilogue_cleanup_slots
        && class_scope_shape(&a.lexical_class) == class_scope_shape(&b.lexical_class)
        && params_equal(&a.params, &b.params)
        && locals_equal(&a.locals, &b.locals)
        && values_equal(&a.values, &b.values)
        && instruction_tables_equal(&a.instructions, &b.instructions)
        && blocks_equal(&a.blocks, &b.blocks)
}

/// Collapses a lexical class scope to its template base, so two instantiations of one generic
/// class compare equal while two different classes never do.
fn class_scope_shape(scope: &Option<String>) -> Option<String> {
    scope
        .as_ref()
        .map(|name| name.split('<').next().unwrap_or(name).to_string())
}

/// Compares the caller-visible parameter metadata, shaping each PHP type.
fn params_equal(a: &[FunctionParam], b: &[FunctionParam]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.name == b.name
                && a.ir_type == b.ir_type
                && shape(&a.php_type) == shape(&b.php_type)
                && a.by_ref == b.by_ref
                && a.variadic == b.variadic
        })
}

/// Compares local slots, ignoring the debug name.
fn locals_equal(a: &[LocalSlot], b: &[LocalSlot]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.id == b.id
                && a.ir_type == b.ir_type
                && shape(&a.php_type) == shape(&b.php_type)
                && a.kind == b.kind
        })
}

/// Compares SSA value metadata (storage type, shaped PHP type, definition site, ownership).
fn values_equal(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.ir_type == b.ir_type
                && shape(&a.php_type) == shape(&b.php_type)
                && a.def == b.def
                && a.ownership == b.ownership
        })
}

/// Compares basic blocks, ignoring the block label (which carries the function's own identity).
fn blocks_equal(a: &[BasicBlock], b: &[BasicBlock]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.id == b.id
                && a.params == b.params
                && a.instructions == b.instructions
                && a.terminator == b.terminator
        })
}

/// Compares instruction streams, ignoring the diagnostic span and pass origin.
///
/// The immediate is compared EXACTLY: a class id, method slot, type name or data id that differs
/// is what refuses a nominal use, so it must not be shaped away.
fn instructions_equal(a: &Instruction, b: &Instruction) -> bool {
    a.op == b.op
        && a.operands == b.operands
        && a.immediate == b.immediate
        && a.result == b.result
        && a.result_type == b.result_type
        && shape(&a.result_php_type) == shape(&b.result_php_type)
        && a.result_ownership == b.result_ownership
        && a.effects == b.effects
}

/// Whether two functions' instruction tables agree element for element.
fn instruction_tables_equal(a: &[Instruction], b: &[Instruction]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| instructions_equal(a, b))
}

/// Rewrites every `FunctionRef` immediate of every function through `resolve`.
fn rewrite_function_refs(module: &mut Module, resolve: &impl Fn(usize) -> Option<u32>) {
    let fix = |function: &mut Function| {
        for instruction in function.instructions.iter_mut() {
            if let Some(Immediate::FunctionRef(id)) = instruction.immediate {
                if let Some(new) = resolve(id.as_raw() as usize) {
                    instruction.immediate = Some(Immediate::FunctionRef(FunctionId::from_raw(new)));
                }
            }
        }
    };
    for function in module.functions.iter_mut() {
        fix(function);
    }
    for function in module.class_methods.iter_mut() {
        fix(function);
    }
    for function in module.closures.iter_mut() {
        fix(function);
    }
    for function in module.callback_wrappers.iter_mut() {
        fix(function);
    }
    for function in module.fiber_wrappers.iter_mut() {
        fix(function);
    }
    for function in module.runtime_callable_invokers.iter_mut() {
        fix(function);
    }
    for function in module.extern_callback_trampolines.iter_mut() {
        fix(function);
    }
}

/// Merges structurally-identical monomorphized METHODS, returning how many were removed.
///
/// A generic class's T-independent method is emitted once per instantiation, under
/// `<class>::<method>`. The duplicate body is dropped and its class's method-impl entry is
/// redirected to the survivor's class, so every vtable, callable and reflection table that
/// resolves a method through `class_info.method_impl_classes` (all keyed by `php_symbol_key`)
/// emits the survivor's symbol instead. This is the choke point that avoids threading a symbol
/// map through the codegen.
pub fn merge_shape_identical_methods(module: &mut Module) -> usize {
    // Candidate methods, each with its extractable instance-method calls. A method whose
    // receiver-typed call cannot be extracted is not a candidate at all.
    let mut candidates: Vec<usize> = Vec::new();
    let mut calls: Vec<Vec<(String, Option<String>, bool)>> = Vec::new();
    for (index, function) in module.class_methods.iter().enumerate() {
        if !method_is_mergeable_candidate(function, &module.data) {
            continue;
        }
        let Some(extracted) = dispatch_calls(function, &module.data) else {
            continue;
        };
        candidates.push(index);
        calls.push(extracted);
    }
    if candidates.len() < 2 {
        return 0;
    }
    // (class, php_symbol_key(method), static) -> candidate position, so a call site can name the
    // candidate it dispatches to.
    let mut node_of: HashMap<(String, String, bool), usize> = HashMap::new();
    for (position, &index) in candidates.iter().enumerate() {
        let function = &module.class_methods[index];
        if let Some((class, method)) = split_method_name(&function.name) {
            node_of.insert(
                (
                    class.to_string(),
                    crate::names::php_symbol_key(method),
                    function.flags.is_static,
                ),
                position,
            );
        }
    }
    // Bucket by dispatch slot (method base + staticness) so only same-slot methods are compared.
    let mut buckets: HashMap<(String, bool), Vec<usize>> = HashMap::new();
    for (position, &index) in candidates.iter().enumerate() {
        let function = &module.class_methods[index];
        if let Some((_, method)) = split_method_name(&function.name) {
            buckets
                .entry((method.to_string(), function.flags.is_static))
                .or_default()
                .push(position);
        }
    }
    let mut parent: Vec<usize> = (0..candidates.len()).collect();
    // Fixed point over ALL slots: union a pair when its bodies match under the object shape AND
    // every method call resolves to the same callee given the merges accepted so far. Merging a
    // callee in one slot unlocks its callers in another, so every slot is re-swept until a sweep
    // changes nothing. A per-slot inner fixpoint would be order-dependent: with two instantiations
    // the caller's bucket can be visited before the callee's and never re-checked.
    loop {
        let mut changed = false;
        for positions in buckets.values() {
            for (offset, &p) in positions.iter().enumerate() {
                for &q in &positions[offset + 1..] {
                    if dsu_find(&mut parent, p) == dsu_find(&mut parent, q) {
                        continue;
                    }
                    let a = &module.class_methods[candidates[p]];
                    let b = &module.class_methods[candidates[q]];
                    if cheap_key(a) != cheap_key(b) || !shape_equal(a, b) {
                        continue;
                    }
                    if !calls_consistent(&calls[p], &calls[q], &node_of, &mut parent) {
                        continue;
                    }
                    let (rp, rq) = (dsu_find(&mut parent, p), dsu_find(&mut parent, q));
                    parent[rp] = rq;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Apply: the survivor of a component is its lowest candidate position.
    let mut survivor_of_root: HashMap<usize, usize> = HashMap::new();
    for position in 0..candidates.len() {
        let root = dsu_find(&mut parent, position);
        survivor_of_root
            .entry(root)
            .and_modify(|existing| {
                if position < *existing {
                    *existing = position;
                }
            })
            .or_insert(position);
    }
    let mut removed: Vec<usize> = Vec::new();
    let mut redirects: Vec<(String, String, bool, String)> = Vec::new();
    for position in 0..candidates.len() {
        let root = dsu_find(&mut parent, position);
        let survivor_position = survivor_of_root[&root];
        if survivor_position == position {
            continue;
        }
        let index = candidates[position];
        let survivor = candidates[survivor_position];
        let (dup_class, method) = split_method_name(&module.class_methods[index].name)
            .expect("a merged method has a class receiver");
        let (survivor_class, _) = split_method_name(&module.class_methods[survivor].name)
            .expect("a surviving method has a class receiver");
        redirects.push((
            dup_class.to_string(),
            method.to_string(),
            module.class_methods[index].flags.is_static,
            survivor_class.to_string(),
        ));
        removed.push(index);
    }
    if removed.is_empty() {
        return 0;
    }
    for (dup_class, method, is_static, survivor_class) in redirects {
        let key = crate::names::php_symbol_key(&method);
        if let Some(info) = module.class_infos.get_mut(&dup_class) {
            let map = if is_static {
                &mut info.static_method_impl_classes
            } else {
                &mut info.method_impl_classes
            };
            // Only an OWN method is redirected; an inherited entry already names an ancestor, and
            // the survivor's class must stay the one that carries the emitted symbol.
            if map.get(&key).map(String::as_str) == Some(dup_class.as_str()) {
                map.insert(key, survivor_class);
            }
        }
    }
    let removed_set: HashSet<usize> = removed.into_iter().collect();
    let count = removed_set.len();
    let mut new_methods: Vec<Function> = Vec::with_capacity(module.class_methods.len() - count);
    for (index, function) in module.class_methods.drain(..).enumerate() {
        if removed_set.contains(&index) {
            continue;
        }
        new_methods.push(function);
    }
    module.class_methods = new_methods;
    count
}

/// Union-find root with path halving.
fn dsu_find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// Returns the dispatch calls of one body as `(method name, receiver class, is static)`.
///
/// An instance call's receiver class comes from the receiver value's static type; a `self::m()`
/// static call resolves through the lexical class, the instantiation the shape erases. A
/// non-object instance receiver is `None`: its dispatch is resolved at run time and is therefore
/// identical across instantiations. `None` for the whole result means a call this pass cannot
/// reason about, so the caller is not a candidate.
fn dispatch_calls(
    function: &Function,
    data: &crate::ir::DataPool,
) -> Option<Vec<(String, Option<String>, bool)>> {
    use crate::ir::Op;
    let mut calls = Vec::new();
    for instruction in &function.instructions {
        let is_static = match instruction.op {
            Op::MethodCall | Op::NullsafeMethodCall => false,
            Op::StaticMethodCall => true,
            _ => continue,
        };
        let target = match instruction.immediate {
            Some(Immediate::Data(id)) | Some(Immediate::ProfiledData { data: id, .. }) => {
                data.strings.get(id.as_raw() as usize)?.clone()
            }
            _ => return None,
        };
        match instruction.op {
            Op::StaticMethodCall => {
                let (receiver, method) = target.rsplit_once("::")?;
                if !receiver.eq_ignore_ascii_case("self") {
                    // A concrete class target is the same class in both bodies, so the call
                    // carries no shape-erased information.
                    calls.push((method.to_string(), Some(receiver.to_string()), is_static));
                    continue;
                }
                let lexical = function.lexical_class.clone()?;
                calls.push((method.to_string(), Some(lexical), is_static));
            }
            _ => {
                let receiver = *instruction.operands.first()?;
                let class = match &function.values.get(receiver.as_raw() as usize)?.php_type {
                    PhpType::Object(name) => Some(name.clone()),
                    _ => None,
                };
                calls.push((target, class, is_static));
            }
        }
    }
    Some(calls)
}

/// Whether two call lists resolve to the same callees under the current merge state.
///
/// `shape_equal` already guarantees the two bodies have the same calls in the same order with the
/// same names, so only the receiver class can differ: an identical class, or two non-object
/// receivers, dispatch identically; two different object classes are safe only when their callees
/// are the same candidate component (already merged).
fn calls_consistent(
    a: &[(String, Option<String>, bool)],
    b: &[(String, Option<String>, bool)],
    node_of: &HashMap<(String, String, bool), usize>,
    parent: &mut [usize],
) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for ((name, recv_a, static_a), (_, recv_b, static_b)) in a.iter().zip(b.iter()) {
        if static_a != static_b {
            return false;
        }
        match (recv_a, recv_b) {
            (Some(class_a), Some(class_b)) if class_a != class_b => {
                let key_a = (class_a.clone(), crate::names::php_symbol_key(name), *static_a);
                let key_b = (class_b.clone(), crate::names::php_symbol_key(name), *static_b);
                match (node_of.get(&key_a).copied(), node_of.get(&key_b).copied()) {
                    (Some(na), Some(nb)) if dsu_find(parent, na) == dsu_find(parent, nb) => {}
                    _ => return false,
                }
            }
            (Some(_), Some(_)) | (None, None) => {}
            _ => return false,
        }
    }
    true
}

/// Returns the `(class, method)` halves of a `Class::method` EIR function name.
fn split_method_name(name: &str) -> Option<(&str, &str)> {
    name.rsplit_once("::")
}

/// Returns whether a class method may be merged away at all.
fn method_is_mergeable_candidate(function: &Function, data: &crate::ir::DataPool) -> bool {
    let Some((class, _)) = split_method_name(&function.name) else {
        return false;
    };
    // Only an instantiated owner is worth merging; an ordinary class keeps its own symbol.
    class.contains('<')
        && !function.flags.is_generator
        && !function.flags.is_synthetic
        && !has_nominal_use(function, data, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Objects collapse to one shape; containers of objects collapse with them; scalars do not.
    #[test]
    fn object_names_collapse_to_one_shape() {
        assert_eq!(
            shape(&PhpType::Object("App\\User".to_string())),
            PhpType::Object(String::new())
        );
        assert_eq!(
            shape(&PhpType::Array(Box::new(PhpType::Object("B".to_string())))),
            PhpType::Array(Box::new(PhpType::Object(String::new()))),
        );
        assert_eq!(
            shape(&PhpType::Union(vec![
                PhpType::Object("A".to_string()),
                PhpType::Int,
            ])),
            PhpType::Union(vec![PhpType::Object(String::new()), PhpType::Int]),
        );
        // A scalar keeps its identity, so `Int` and `Bool` never collapse together.
        assert_ne!(shape(&PhpType::Int), shape(&PhpType::Bool));
        // `False` and `Never` are return-type markers the backend never consults on their own, so
        // they collapse onto the type the backend actually reads.
        assert_eq!(shape(&PhpType::False), PhpType::Bool);
        assert_eq!(shape(&PhpType::Never), PhpType::Void);
        // A `Resource` keeps its identity: the conversions read it exactly.
        assert_ne!(shape(&PhpType::Resource(Some("gd".to_string()))), PhpType::Int);
    }
}
