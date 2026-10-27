//! Purpose:
//! Resolves statically bound instance-method call sites for the small-function inliner and
//! selects the method bodies whose splice is provably equivalent to the real call.
//!
//! Called from:
//! - `super::inline_small_functions`, once per inliner round, before any host is mutated.
//!
//! Key details:
//! - A `method_call` is bound statically only when codegen itself would not dispatch through
//!   the vtable to a different body: the receiver's static class is final, the method is final,
//!   or the class has no vtable slot for it. Instantiated generic receivers with a vtable slot
//!   keep a real call, since variance can change the actual instantiation without inheritance.
//!   The selected body is the one
//!   `method_resolution::resolve_method_call_target` would call directly.
//! - A spliced body loses its own frame, so only bodies whose lowering cannot observe that are
//!   admitted: every instruction is in `op_is_frame_independent`, whose codegen never reads the
//!   enclosing function's name, flags or signature. Scope-sensitive property and call operations
//!   keep running in the method's class through `Function::inlined_scopes`.
//! - Property accesses and nested method calls are limited to `$this` and packed records:
//!   neither receiver can be null, so
//!   the read can never reach the null-receiver warning, which would otherwise report the line
//!   of whatever call the host published last instead of the method call's.
//! - Modules that can request a backtrace directly or through the eval bridge
//!   (`Module::calls_php_backtrace`) keep every method call, and a body that keeps a backtrace
//!   argument snapshot (every frame does when a backtrace is reachable dynamically) is never a
//!   candidate, so a method frame a backtrace could list always stays a real frame.
//! - The receiver null check the real call performs is reproduced at the splice by the caller
//!   (`super::apply_inline_at_site`); this module only reports when it can be skipped.

use std::collections::{HashMap, HashSet};

use crate::ir::{
    Function, Immediate, Instruction, IrHeapKind, IrType, LocalSlotId, Module, Op, ValueDef,
    ValueId,
};
use crate::names::php_symbol_key;
use crate::types::{FunctionSig, PhpType};

/// Name of the receiver parameter every instance method body declares first.
const RECEIVER_NAME: &str = "this";

/// Returns the receiver parameter's slot; EIR binds parameters to the first local slots in order.
fn receiver_slot() -> LocalSlotId {
    LocalSlotId::from_raw(0)
}

/// Statically bound method call targets and the candidate bodies for one inliner round.
pub(super) struct MethodTargets {
    /// Pre-mutation snapshots of the candidate method bodies.
    bodies: Vec<Function>,
    /// `(receiver class, PHP method key)` to an index into `bodies`.
    targets: HashMap<(String, String), usize>,
}

impl MethodTargets {
    /// Builds the target table, empty when the module can request a backtrace.
    pub(super) fn new(module: &Module) -> Self {
        let mut table = MethodTargets {
            bodies: Vec::new(),
            targets: HashMap::new(),
        };
        if module.calls_php_backtrace() {
            return table;
        }
        let mut body_by_implementation: HashMap<(String, String), usize> = HashMap::new();
        for (index, function) in module.class_methods.iter().enumerate() {
            if !body_is_candidate(module, function) {
                continue;
            }
            let Some((class, method)) = function.name.rsplit_once("::") else {
                continue;
            };
            body_by_implementation.insert((class.to_string(), php_symbol_key(method)), index);
        }
        if body_by_implementation.is_empty() {
            return table;
        }
        let mut snapshot_of_body: HashMap<usize, usize> = HashMap::new();
        for (class_name, class_info) in &module.class_infos {
            if class_info.is_internal
                || module.interface_infos.contains_key(class_name)
                || module.enum_infos.contains_key(class_name)
            {
                continue;
            }
            for (method_key, signature) in &class_info.methods {
                if !signature_is_candidate(method_key, signature) {
                    continue;
                }
                // Variance can widen an instantiated receiver without an inheritance edge.
                let dispatches_dynamically = class_info.vtable_slots.contains_key(method_key)
                    && (class_name.contains('<')
                        || (!class_info.final_methods.contains(method_key) && !class_info.is_final));
                if dispatches_dynamically {
                    continue;
                }
                let implementation = class_info
                    .method_impl_classes
                    .get(method_key)
                    .cloned()
                    .unwrap_or_else(|| class_name.clone());
                if module
                    .class_infos
                    .get(&implementation)
                    .is_none_or(|info| info.is_internal)
                {
                    continue;
                }
                let Some(&body) = body_by_implementation.get(&(implementation, method_key.clone()))
                else {
                    continue;
                };
                let snapshot = *snapshot_of_body.entry(body).or_insert_with(|| {
                    let mut snapshot = module.class_methods[body].clone();
                    preserve_host_concat_scratch(&mut snapshot);
                    table.bodies.push(snapshot);
                    table.bodies.len() - 1
                });
                table
                    .targets
                    .insert((class_name.clone(), method_key.clone()), snapshot);
            }
        }
        table
    }

    /// Returns the candidate method bodies, indexed by `resolve`'s answers.
    pub(super) fn bodies(&self) -> &[Function] {
        &self.bodies
    }

    /// Resolves a `method_call` on a plain object receiver to a candidate body index.
    ///
    /// `strings` is the module literal pool the call's method-name immediate indexes.
    pub(super) fn resolve(
        &self,
        host: &Function,
        inst: &Instruction,
        strings: &[String],
    ) -> Option<usize> {
        if inst.op != Op::MethodCall || self.targets.is_empty() {
            return None;
        }
        let Some(Immediate::Data(method)) = inst.immediate else {
            return None;
        };
        let method = strings.get(method.as_raw() as usize)?;
        let receiver = host.value(*inst.operands.first()?)?;
        // A nullable or union receiver takes codegen's guarded dispatch paths instead.
        let PhpType::Object(class_name) = &receiver.php_type else {
            return None;
        };
        if class_name.is_empty() || receiver.ir_type != IrType::Heap(IrHeapKind::Object) {
            return None;
        }
        let body = self.targets.get(&(
            class_name.trim_start_matches('\\').to_string(),
            php_symbol_key(method),
        )).copied()?;
        // Initializer-only property-store compatibility must never apply to a spliced setter.
        // Same-class splices intentionally have no scope-map entry to distinguish their origin.
        if host.name.starts_with("_class_propinit_")
            && self.bodies[body].instructions.iter()
                .any(|inst| matches!(inst.op, Op::PropSet | Op::PropGetForWrite))
        {
            return None;
        }
        Some(body)
    }
}

/// Returns the method name a `method_call` spells, for its null-receiver error message.
pub(super) fn call_method_name<'a>(inst: &Instruction, strings: &'a [String]) -> Option<&'a str> {
    let Some(Immediate::Data(method)) = inst.immediate else {
        return None;
    };
    strings.get(method.as_raw() as usize).map(String::as_str)
}

/// Returns whether `receiver` is the host method's own `$this`, which PHP never binds to null.
///
/// Only the host's receiver slot read directly qualifies: PHP rejects assignments to `$this`,
/// so nothing but the prologue ever writes that slot.
pub(super) fn receiver_is_host_this(host: &Function, receiver: ValueId) -> bool {
    let host_has_receiver = host.flags.is_method
        && !host.flags.is_static
        && !host.flags.is_closure
        && host.params.first().is_some_and(|param| param.name == RECEIVER_NAME)
        && host
            .locals
            .first()
            .is_some_and(|local| local.name.as_deref() == Some(RECEIVER_NAME));
    host_has_receiver && value_loads_slot(host, receiver, receiver_slot())
}

/// Returns whether the method signature can be bound without the caller-side guards a real
/// call performs: no by-reference or variadic parameter, no literal `false`/`null` parameter
/// guard, no surplus-argument ABI, no by-reference return, no deprecation notice, and no
/// magic method.
fn signature_is_candidate(method_key: &str, signature: &FunctionSig) -> bool {
    !method_key.starts_with("__")
        && !signature.by_ref_return
        && signature.variadic.is_none()
        && signature.deprecation.is_none()
        && !signature.ref_params.iter().any(|by_ref| *by_ref)
        && !signature
            .params
            .iter()
            .any(|(_, declared)| matches!(declared, PhpType::False | PhpType::Void))
        && !crate::func_args::sig_collects_surplus_args(signature)
        && !crate::func_args::sig_has_hidden_argc_param(signature)
}

/// Returns whether one lowered method body may be spliced into another function.
///
/// The body-independent rules (size, recursion, ownership, loops, return shape) are the
/// ordinary inliner rules applied per site; this adds what only a method needs.
fn body_is_candidate(module: &Module, function: &Function) -> bool {
    let flags = &function.flags;
    if !flags.is_method
        || flags.is_static
        || flags.is_closure
        || flags.is_generator
        || flags.is_fiber_wrapper
        || flags.is_callback_wrapper
        || flags.is_runtime_callable_invoker
        || flags.by_ref_return
        || flags.is_internal
        || flags.is_synthetic
        || flags.is_main
    {
        return false;
    }
    let Some(class_name) = function.lexical_class.as_deref() else {
        return false;
    };
    let Some(receiver) = function.params.first() else {
        return false;
    };
    if receiver.name != RECEIVER_NAME
        || receiver.ir_type != IrType::Heap(IrHeapKind::Object)
        || !matches!(&receiver.php_type, PhpType::Object(name) if name == class_name)
        || !function
            .locals
            .first()
            .is_some_and(|local| local.name.as_deref() == Some(RECEIVER_NAME))
    {
        return false;
    }
    if function.keeps_backtrace_argument_snapshot()
        || function.params[1..]
            .iter()
            .any(|param| matches!(param.php_type, PhpType::False | PhpType::Void))
    {
        return false;
    }
    if !super::callee_fits_budget(function, true) {
        return false;
    }
    let scoped_temps = super::temporaries::scoped_temps(function);
    function
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .filter_map(|&inst| function.instruction(inst))
        .all(|inst| instruction_is_frame_independent(module, function, inst, &scoped_temps))
}

/// Resolves a user static method with an explicit class, excluding lexical and runtime intrinsics.
pub(super) fn explicit_static_method_name(module: &Module, inst: &Instruction) -> Option<String> {
    if inst.op != Op::StaticMethodCall { return None; }
    let Some(Immediate::Data(id)) = inst.immediate else { return None; };
    let (class, method) = module.data.strings.get(id.as_raw() as usize)?.rsplit_once("::")?;
    let class = class.trim_start_matches('\\');
    if matches!(class, "self" | "parent" | "static") { return None; }
    let info = module.class_infos.get(class)?;
    if info.is_internal { return None; }
    let key = php_symbol_key(method);
    info.static_methods.get(&key)?;
    let implementation = info.static_method_impl_classes.get(&key).map(String::as_str).unwrap_or(class);
    if module.class_infos.get(implementation)?.is_internal { return None; }
    Some(format!("{implementation}::{key}"))
}

/// Resolves statically bound instance bodies for cycle detection even when they cannot be inlined.
pub(super) fn bound_method_name(module: &Module, function: &Function, inst: &Instruction) -> Option<String> {
    if inst.op != Op::MethodCall { return None; }
    let Some(Immediate::Data(id)) = inst.immediate else { return None; };
    let method = module.data.strings.get(id.as_raw() as usize)?;
    let PhpType::Object(class) = &function.value(*inst.operands.first()?)?.php_type else { return None; };
    let class = class.trim_start_matches('\\');
    let info = module.class_infos.get(class)?;
    if info.is_internal || module.interface_infos.contains_key(class) { return None; }
    let key = php_symbol_key(method);
    if info.vtable_slots.contains_key(&key)
        && (class.contains('<') || (!info.is_final && !info.final_methods.contains(&key)))
    {
        return None;
    }
    info.methods.get(&key)?;
    let implementation = info.method_impl_classes.get(&key).map(String::as_str).unwrap_or(class);
    Some(format!("{implementation}::{key}"))
}

/// Returns whether a method-body instruction lowers the same in any enclosing function.
fn instruction_is_frame_independent(
    module: &Module, function: &Function, inst: &Instruction,
    scoped_temps: &HashSet<LocalSlotId>,
) -> bool {
    if inst.op == Op::ObjectNew {
        return constructs_builtin_throwable(module, inst);
    }
    if matches!(inst.op, Op::PushCallOperandOwner | Op::PopCallOperandOwner
        | Op::UnsetLocal | Op::ReleaseLocalSlot)
    {
        return matches!(inst.immediate, Some(Immediate::LocalSlot(slot)) if scoped_temps.contains(&slot));
    }
    if !op_is_frame_independent(inst.op) { return false; }
    if store_resets_concat_scratch(function, inst)
        && !matches!(inst.immediate, Some(Immediate::LocalSlot(slot)) if scoped_temps.contains(&slot))
    { return false; }
    if inst.op == Op::StaticMethodCall {
        return explicit_static_method_name(module, inst).is_some();
    }
    if !matches!(inst.op, Op::PropGet | Op::PropSet | Op::PropGetForWrite | Op::MethodCall) {
        return true;
    }
    if !matches!(inst.immediate, Some(Immediate::Data(_))) { return false; }
    function.is_packed_field_access(inst)
        || inst.operands.first().is_some_and(|&receiver| {
            value_loads_slot(function, receiver, receiver_slot())
        })
}

/// Recognizes the source store for which codegen implicitly rewinds its own frame's string scratch.
///
/// `local_stores::lower_store_local` resets only positive-span `StoreLocal(Acquire(StrConcat))`.
/// The normal splice neutralizes explicit `ConcatReset` but cannot let this store rewind the host.
fn store_resets_concat_scratch(function: &Function, inst: &Instruction) -> bool {
    if inst.op != Op::StoreLocal || !inst.span.is_some_and(|span| span.line > 0) { return false; }
    let Some(acquire) = inst.operands.first().and_then(|&value| defining_instruction(function, value))
    else { return false; };
    acquire.op == Op::Acquire && acquire.operands.first()
        .and_then(|&value| defining_instruction(function, value))
        .is_some_and(|source| source.op == Op::StrConcat)
}

/// Returns an SSA value's instruction without treating block parameters as instruction results.
fn defining_instruction(function: &Function, value: ValueId) -> Option<&Instruction> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else { return None; };
    function.instruction(inst)
}

/// Returns whether an opcode's codegen never consults the enclosing function's identity.
///
/// These ops lower from operands, immediates, module metadata and the recorded lexical class.
/// Explicit static calls are checked separately; lexical `self`/`parent`/`static` calls, static
/// locals, `func_get_args()`, backtraces and eval scopes remain excluded. Nested calls retain
/// their instruction spans, so each call still publishes its own source line before execution.
fn op_is_frame_independent(op: Op) -> bool {
    matches!(
        op,
        Op::Nop
            | Op::ConcatReset
            | Op::ConstI64
            | Op::ConstF64
            | Op::ConstStr
            | Op::ConstNull
            | Op::ConstBool
            | Op::LoadLocal
            | Op::StoreLocal
            | Op::IAdd
            | Op::ISub
            | Op::IMul
            | Op::ICheckedAdd
            | Op::ICheckedSub
            | Op::ICheckedMul
            | Op::ICheckedAddToInt
            | Op::ICheckedSubToInt
            | Op::ICheckedMulToInt
            | Op::INeg
            | Op::IBitAnd
            | Op::IBitOr
            | Op::IBitXor
            | Op::IBitNot
            | Op::IShl
            | Op::IShrA
            | Op::FAdd
            | Op::FSub
            | Op::FMul
            | Op::FDiv
            | Op::FNeg
            | Op::ICmp
            | Op::FCmp
            | Op::StrictEq
            | Op::StrictNotEq
            | Op::IsNull
            | Op::IsTruthy
            | Op::IToF
            | Op::IToStr
            | Op::Acquire
            | Op::Release
            | Op::Move
            | Op::Borrow
            | Op::BufferGet
            | Op::BufferLen
            | Op::PropGet
            | Op::PropGetForWrite
            | Op::PropSet
            | Op::Call
            | Op::FunctionVariantCall
            | Op::MethodCall
            | Op::StaticMethodCall
            | Op::StrConcat
            | Op::StrAppend
            | Op::StrLen
            | Op::StrPersist
            | Op::ReturnBoundaryMixedToInt
            // A message fixed at lowering, such as a receiver null check an earlier round
            // spliced into this body: `emit_error` reads nothing from the frame.
            | Op::ThrowError
    )
}

/// Returns whether `value` reads local `slot`, directly or through ownership forwarding.
fn value_loads_slot(function: &Function, value: ValueId, slot: LocalSlotId) -> bool {
    let mut current = value;
    for _ in 0..8 {
        let Some(ValueDef::Instruction { inst, .. }) = function.value(current).map(|value| value.def)
        else {
            return false;
        };
        let Some(inst) = function.instruction(inst) else {
            return false;
        };
        match inst.op {
            Op::LoadLocal => return inst.immediate == Some(Immediate::LocalSlot(slot)),
            Op::Move | Op::Borrow => match inst.operands.first() {
                Some(&source) => current = source,
                None => return false,
            },
            _ => return false,
        }
    }
    false
}

/// Only checker-provided Throwable allocations use the frame-independent compact constructor.
/// User constructors, reflection and other allocation intrinsics retain their real method frame.
fn constructs_builtin_throwable(module: &Module, inst: &Instruction) -> bool {
    let Some(Immediate::Data(id)) = inst.immediate else { return false; };
    let Some(name) = module.data.class_names.get(id.as_raw() as usize) else { return false; };
    let Some(info) = module.class_infos.get(name) else { return false; };
    if info.method_declaring_classes.get("__construct")
        .is_none_or(|owner| !matches!(owner.as_str(), "Exception" | "Error"))
        || info.property_declaring_classes.values()
            .any(|owner| !matches!(owner.as_str(), "Exception" | "Error"))
    { return false; }
    let mut name = name.as_str();
    for _ in 0..module.class_infos.len() {
        let Some(info) = module.class_infos.get(name) else { return false; };
        if !info.is_internal { return false; }
        if matches!(name, "Exception" | "Error") { return true; }
        let Some(parent) = info.parent.as_deref() else { return false; };
        name = parent;
    }
    false
}

/// Persists rooted concat arguments without letting their stores rewind the caller's scratch.
/// String Acquire and StrPersist both call the same persistence helper. Using StrPersist for
/// these scoped owners preserves the ownership operation and source span, while preventing
/// local-store codegen's implicit Acquire(StrConcat) reset. Ordinary PHP assignments stay gated.
fn preserve_host_concat_scratch(function: &mut Function) {
    let scoped_temps = super::temporaries::scoped_temps(function);
    let acquires: Vec<_> = function.instructions.iter()
        .filter(|inst| store_resets_concat_scratch(function, inst))
        .filter(|inst| matches!(inst.immediate, Some(Immediate::LocalSlot(slot)) if scoped_temps.contains(&slot)))
        .filter_map(|inst| function.value(*inst.operands.first()?).map(|value| value.def))
        .filter_map(|definition| match definition { ValueDef::Instruction { inst, .. } => Some(inst), _ => None })
        .collect();
    for id in acquires {
        let inst = function.instruction_mut(id).expect("scoped string acquire");
        inst.op = Op::StrPersist;
        inst.effects = Op::StrPersist.default_effects();
    }
}
