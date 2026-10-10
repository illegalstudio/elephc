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
        && !has_nominal_use(function)
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
/// immediate — method dispatch and dynamic property/object access — because two instantiations
/// would then silently share one class's resolution. Property and `instanceof` ops normally carry
/// their class/property as an immediate and are already caught, but they are listed too: the
/// guard must not depend on which of the two encodings an op currently uses.
fn has_nominal_use(function: &Function) -> bool {
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
        matches!(
            instruction.op,
            Op::MethodCall
                | Op::NullsafeMethodCall
                | Op::StaticMethodCall
                | Op::EvalStaticMethodCall
                | Op::IteratorMethodCall
                | Op::SplRuntimeCall
                | Op::MethodLookup
                | Op::DynamicPropGet
                | Op::DynamicPropSet
                | Op::DynamicPropUnset
                | Op::PropGet
                | Op::PropGetForWrite
                | Op::PropInitialized
                | Op::PropSet
                | Op::PropUnset
                | Op::NullsafePropGet
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
    let candidates: Vec<usize> = module
        .class_methods
        .iter()
        .enumerate()
        .filter(|(_, function)| method_is_mergeable_candidate(function))
        .map(|(index, _)| index)
        .collect();
    if candidates.len() < 2 {
        return 0;
    }
    let mut buckets: HashMap<(String, bool), Vec<usize>> = HashMap::new();
    for index in candidates {
        let Some((_, method)) = split_method_name(&module.class_methods[index].name) else {
            continue;
        };
        buckets
            .entry((method.to_string(), module.class_methods[index].flags.is_static))
            .or_default()
            .push(index);
    }
    let mut removed: Vec<usize> = Vec::new();
    let mut redirects: Vec<(String, String, bool, String)> = Vec::new();
    for indices in buckets.values() {
        let mut survivors: Vec<usize> = Vec::new();
        for &index in indices {
            let survivor = survivors.iter().copied().find(|&candidate| {
                shape_equal(&module.class_methods[candidate], &module.class_methods[index])
            });
            match survivor {
                Some(survivor) => {
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
                None => survivors.push(index),
            }
        }
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

/// Returns the `(class, method)` halves of a `Class::method` EIR function name.
fn split_method_name(name: &str) -> Option<(&str, &str)> {
    name.rsplit_once("::")
}

/// Returns whether a class method may be merged away at all.
fn method_is_mergeable_candidate(function: &Function) -> bool {
    let Some((class, _)) = split_method_name(&function.name) else {
        return false;
    };
    // Only an instantiated owner is worth merging; an ordinary class keeps its own symbol.
    class.contains('<')
        && !function.flags.is_generator
        && !function.flags.is_synthetic
        && !has_nominal_use(function)
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
    }
}
