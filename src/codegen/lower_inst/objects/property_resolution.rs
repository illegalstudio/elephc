//! Purpose:
//! Resolves declared property slots and receiver source types.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Case-insensitive class lookup and packed/runtime storage overrides remain authoritative.
//! - A by-name dispatch asks `crate::types::resolve_property_name` first: the physical slot table
//!   still carries a strict ancestor's private slot under its plain name, which php resolves to a
//!   DYNAMIC property everywhere except inside the class that declared it.

use super::*;

use crate::types::PropertyNameResolution;

/// Allocates an object-owned ref cell before a physical initializer writes its default.
pub(super) fn initialize_owned_property_reference(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    base_reg: &str,
) -> bool {
    let owns_cell = slot.is_reference && ctx.module.class_infos.get(&slot.class_name)
        .is_some_and(|class| class.owned_reference_properties.contains(&slot.property));
    if owns_cell {
        emit_owned_reference_property_cell(ctx, base_reg, slot.offset, &slot.php_type, false);
    }
    owns_cell
}

/// Resolves a physical initializer slot without applying name-based shadow selection.
pub(super) fn resolve_initializer_property_slot(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    class_id: u32,
    index: u32,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let PhpType::Object(class_name) = ctx.value_php_type(object)?.codegen_repr() else {
        return Err(CodegenIrError::invalid_module("property initializer needs a concrete object"));
    };
    let info = ctx.module.class_infos.get(class_name.trim_start_matches('\\'))
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown initializer class {class_name}")))?;
    if info.class_id != u64::from(class_id)
        || !ctx.function.flags.is_synthetic
        || ctx.function.name != format!("_class_propinit_{class_id}")
    {
        return Err(CodegenIrError::invalid_module("physical property reference outside its initializer"));
    }
    resolve_physical_property_slot(ctx, object, class_id, index, inst)
}

/// Resolves a compiler-trusted physical property slot by class id and layout index.
///
/// Reflection uses this path because PHP reflection intentionally bypasses caller visibility.
/// The receiver must still be a concrete instance of the referenced class or one of its
/// subclasses. Ordinary source property access continues through the scope-aware name resolver.
pub(super) fn resolve_physical_property_slot(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    class_id: u32,
    index: u32,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let PhpType::Object(receiver_class) = ctx.value_php_type(object)?.codegen_repr() else {
        return Err(CodegenIrError::invalid_module(
            "physical property access needs a concrete object",
        ));
    };
    let Some((class_name, info)) = ctx
        .module
        .class_infos
        .iter()
        .find(|(_, info)| info.class_id == u64::from(class_id))
    else {
        return Err(CodegenIrError::invalid_module(format!(
            "physical property access references unknown class id {class_id}",
        )));
    };
    if !class_extends_class(ctx, &receiver_class, class_name) {
        return Err(CodegenIrError::invalid_module(format!(
            "physical property access for {class_name} cannot use receiver {receiver_class}",
        )));
    }
    let index = index as usize;
    let (property, php_type) = info.properties.get(index)
        .ok_or_else(|| CodegenIrError::invalid_module("physical property index is outside the class layout"))?;
    ensure_property_type_supported(php_type, inst)?;
    Ok(PropertySlot {
        class_name: class_name.clone(),
        property: property.clone(),
        php_type: php_type.clone(),
        offset: 8 + index * 16,
        is_declared: info.property_slot_is_declared(index, property),
        is_packed: false,
        is_reference: info.property_slot_is_reference(index, property),
    })
}

/// Resolves the property slot for a concrete object receiver and declared property name.
pub(super) fn resolve_property_slot(
    ctx: &FunctionContext<'_>,
    object: crate::ir::ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let object_ty = ctx.value_php_type(object)?;
    let PhpType::Object(class_name) = object_ty else {
        if let PhpType::Packed(class_name) = object_ty {
            return resolve_packed_field_slot(ctx, &class_name, property, inst);
        }
        return Err(CodegenIrError::unsupported(format!(
            "{} for receiver PHP type {:?}",
            inst.op.name(),
            object_ty
        )));
    };
    resolve_property_slot_for_class(ctx, &class_name, property, inst)
}

/// What php does with ONE property name on ONE RUNTIME class, seen from the current scope.
///
/// A receiver's STATIC class only bounds its runtime class, and under that bound php's answer can
/// change KIND, not merely address. Measured on php 8.5.10:
/// `class A { private $p; } class P extends A {} class Q extends P { public $p; }` reached through
/// `function f(P $x)` answers from a DYNAMIC property on a `P` and from `Q`'s own public SLOT on a
/// `Q`. A subclass can also widen a `protected` its parent refuses, declare an accessor its parent
/// does not, and lay its per-instance hash out at a different offset because it declares more
/// properties. So the dispatch has to pick the whole action per runtime class.
pub(super) enum PropertyRuntimeAction {
    /// The name addresses this physical slot on this runtime class.
    Slot(PropertySlot),
    /// php answers from THIS class's per-instance hash, at THIS class's offset, and names THIS
    /// class in the creation notice or the `Undefined property` warning.
    DynamicHash {
        /// This class's own `8 + slots * 16` hash offset.
        hash_offset: usize,
        /// Whether a value read that finds no entry reports `Undefined property`.
        ///
        /// Phase B1 established that a strict ancestor's private name DOES report it, because php
        /// does. An ordinary undeclared name on an `#[\AllowDynamicProperties]` class keeps the
        /// answer this backend already gave it, which is silence: changing that is general
        /// undefined-property warning work and is deliberately not part of this phase.
        warns_on_miss: bool,
    },
    /// php answers dynamically but this class reserves no hash, so the entry can never exist.
    ///
    /// A value read warns `Undefined property` and answers null, a probe answers null in silence,
    /// an `unset()` is a no-op. A WRITE must never reach this: `scope_dynamic_storage` reserves
    /// the hash for exactly the classes a reachable mutation can address, so a write arm carrying
    /// it is a compiler invariant failure rather than a program error, and the write sites say so
    /// instead of silently dropping the value.
    DynamicMissing {
        /// Whether a value read reports `Undefined property`. Same rule as `DynamicHash`.
        warns_on_miss: bool,
    },
    /// php refuses the access from this scope: raise this catchable `Error` and touch nothing.
    Refuse {
        /// php 8.5's verbatim wording, e.g. `Cannot access private property D::$n`.
        message: String,
    },
    /// php answers `__get` on THIS runtime class, and the name is a compile-time constant, so the
    /// dispatch makes the real call through `lower_magic_get_prop`.
    ///
    /// Only a value read builds this. A silent probe would consult `__isset`, which has no codegen
    /// call site yet and keeps `MagicDeferred`; a write and an `unset()` are peeled off earlier by
    /// `crate::ir_lower`, which calls `__set` and `__unset` through the ordinary method lowering.
    MagicGet,
    /// php answers an accessor this dispatch does not call.
    ///
    /// For a DIRECT name that never happens here: `crate::ir_lower::stmt::instance_property_writes`
    /// and its read and unset siblings peel such a runtime class off with an `instanceof` guard
    /// and call the real `__get`, `__set` or `__unset` through the ordinary method-call lowering,
    /// so this arm is unreachable by construction and answers php null only so that a future
    /// divergence between the two predicates can never become a storage escape.
    /// For a RUNTIME name it is phase B1's `MagicDeferred`, still deferred, still never a slot.
    MagicDeferred,
}

/// One runtime-class arm of a property dispatch.
pub(super) struct PropertyRuntimeArm {
    /// Runtime class id this arm matches, or `None` for the static class's fallthrough arm.
    pub(super) class_id: Option<u64>,
    /// That class's name, which is the one php reports in a notice or a warning.
    pub(super) class_name: String,
    /// What php does on that class.
    pub(super) action: PropertyRuntimeAction,
}

/// Where one property access really lands, per runtime class.
pub(super) enum PropertyRuntimePlan {
    /// The receiver's runtime class is PROVABLY its static class: one action, no comparison.
    Fixed(PropertyRuntimeArm),
    /// Arms in class-id order with the receiver's own static class LAST as the fallthrough.
    ByClassId(Vec<PropertyRuntimeArm>),
}

/// Which operation is asking, and whether the property NAME is a compile-time constant.
///
/// The direct/runtime split is what decides the accessor arm. php consults `__get`, `__set`,
/// `__isset` or `__unset` for a name it does not resolve to a visible slot, and a DIRECT name is
/// answered upstream in `crate::ir_lower` by a real call. Runtime writes defer `__set` to the
/// write-site lowering, which owns the receiver/name reentrancy guard.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum PropertyAccessKind {
    /// `$o->name`, with php's fetch mode.
    DirectRead(PropertyFetchMode),
    /// `$o->{$k}` after the name matched, with php's fetch mode.
    RuntimeRead(PropertyFetchMode),
    /// `$o->name = v`.
    DirectWrite,
    /// `$o->{$k} = v` after the name matched.
    RuntimeWrite,
    /// `unset($o->name)`.
    DirectUnset,
    /// `unset($o->{$k})` after the name matched.
    RuntimeUnset,
    /// `$x = &$o->name`, a by-reference argument, or a by-reference return.
    Reference,
    /// The MISS arm of a runtime-name ladder, where the name matched no declared name at all.
    ///
    /// Only the runtime class's per-instance hash can answer such a name, and no accessor
    /// decision belongs here: the ladder's own arms already made that decision for every name the
    /// class declares, and a name it does not declare is one this phase cannot hand to an
    /// accessor anyway. So this kind asks only WHERE the hash is, per runtime class.
    RuntimeHashMiss,
}

impl PropertyAccessKind {
    /// Returns the accessor php consults before reporting anything, when there is one.
    fn magic_method(self) -> Option<&'static str> {
        match self {
            Self::DirectRead(mode) | Self::RuntimeRead(mode) => {
                Some(if mode.is_read() { "__get" } else { "__isset" })
            }
            Self::DirectWrite | Self::RuntimeWrite => Some("__set"),
            Self::DirectUnset | Self::RuntimeUnset => Some("__unset"),
            // php does not route a reference binding through an accessor: an accessor returns a
            // value, not storage, so there is nothing for the alias to point at. A ladder miss
            // arm deliberately makes no accessor decision either.
            Self::Reference | Self::RuntimeHashMiss => None,
        }
    }

    /// Returns whether this operation can CREATE a dynamic property, which php forbids on a
    /// `readonly` class. The name is known for both write forms, so the refusal can name it.
    fn creates_dynamic_property(self) -> bool {
        matches!(self, Self::DirectWrite | Self::RuntimeWrite)
    }
}

impl PropertyRuntimePlan {
    /// Returns the single arm when the runtime class is provably the static one.
    pub(super) fn fixed_arm(&self) -> Option<&PropertyRuntimeArm> {
        match self {
            Self::Fixed(arm) => Some(arm),
            Self::ByClassId(_) => None,
        }
    }

    /// Returns every arm, for callers that have to validate what the plan can do before it runs.
    pub(super) fn arms(&self) -> &[PropertyRuntimeArm] {
        match self {
            Self::Fixed(arm) => std::slice::from_ref(arm),
            Self::ByClassId(arms) => arms,
        }
    }
}


/// Resolves what one property access does on a named receiver class, per runtime class.
///
/// `Fixed` is claimed only under a proof, never as an optimization. The proof has two parts and
/// both must hold:
///
/// 1. No class in `ctx.module.class_infos` inherits from the receiver's static class. That map is
///    the AOT model's complete set of declared classes, and it is the same map
///    `crate::codegen::eval_property_helpers::dynamic_properties` dispatches its hash-slot helper
///    over, so agreeing with it keeps the two answers consistent.
/// 2. The static class does not carry `eval_property_storage`. That flag is exactly the marker
///    `crate::ir_lower::program::metadata::reserve_eval_subclass_property_storage` sets on every
///    non-final user class when an opaque `eval` is reachable, which is to say the marker for
///    "opaque eval may declare a subclass of this class that the module never enumerated". Under
///    it the runtime class is NOT proven, so the plan stays a ladder whose fallthrough addresses
///    the static class's own layout, which is precisely the layout the eval bridge allocates for
///    such a subclass.
pub(super) fn resolve_property_runtime_plan(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    kind: PropertyAccessKind,
    inst: &Instruction,
) -> Result<PropertyRuntimePlan> {
    let normalized = class_name.trim_start_matches('\\');
    let static_arm = PropertyRuntimeArm {
        class_id: None,
        class_name: normalized.to_string(),
        action: resolve_property_runtime_action(ctx, normalized, property, kind, inst)?,
    };
    if is_builtin_stdclass(normalized) {
        return Ok(PropertyRuntimePlan::Fixed(static_arm));
    }
    let eval_may_subclass = ctx
        .module
        .class_infos
        .get(normalized)
        .is_some_and(|class_info| class_info.eval_property_storage);
    let mut arms = Vec::new();
    for (candidate, candidate_info) in &ctx.module.class_infos {
        if candidate.as_str() == normalized
            || !crate::types::class_inherits_from(&ctx.module.class_infos, candidate, normalized)
        {
            continue;
        }
        arms.push(PropertyRuntimeArm {
            class_id: Some(candidate_info.class_id),
            class_name: candidate.clone(),
            action: resolve_property_runtime_action(ctx, candidate, property, kind, inst)?,
        });
    }
    if arms.is_empty() && !eval_may_subclass {
        return Ok(PropertyRuntimePlan::Fixed(static_arm));
    }
    arms.sort_by_key(|arm| arm.class_id);
    arms.push(static_arm);
    Ok(PropertyRuntimePlan::ByClassId(arms))
}

/// Resolves php's answer for one name on ONE class, which is the per-arm decision.
///
/// The three existing per-class arm resolvers stay the single authority for php's answer; this
/// only turns their answer into the action the ladder emits, and adds the hash offset, which is
/// the one part that is per class rather than per name.
pub(super) fn resolve_property_runtime_action(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    kind: PropertyAccessKind,
    inst: &Instruction,
) -> Result<PropertyRuntimeAction> {
    let resolution = resolve_property_name_in_current_scope(ctx, class_name, property);
    // php consults the accessor BEFORE it reports anything, for every name it does not resolve to
    // a visible slot. The scope-selected private slot of `ScopePrivate` IS visible to this scope,
    // so it keeps the slot exactly as php does.
    if !matches!(
        resolution,
        PropertyNameResolution::Visible | PropertyNameResolution::ScopePrivate { .. }
    ) {
        if let Some(magic) = kind.magic_method() {
            if class_declares_method(ctx, class_name, magic) {
                return Ok(
                    if matches!(kind, PropertyAccessKind::DirectRead(mode) if mode.is_read()) {
                        PropertyRuntimeAction::MagicGet
                    } else {
                        PropertyRuntimeAction::MagicDeferred
                    },
                );
            }
        }
    }
    match resolution {
        PropertyNameResolution::Visible => {
            resolve_property_slot_for_class(ctx, class_name, property, inst)
                .map(PropertyRuntimeAction::Slot)
        }
        PropertyNameResolution::ScopePrivate { scope, index } => {
            resolve_scope_private_property_slot(ctx, &scope, index, property, inst)
                .map(PropertyRuntimeAction::Slot)
        }
        PropertyNameResolution::Dynamic => {
            // A reference binding into the per-instance hash is the capability the dedicated
            // dynamic-reference phase adds. Until then the honest answer is "no storage here",
            // which every reference site turns into a refusal to compile rather than a pointer.
            if kind == PropertyAccessKind::Reference {
                return Ok(PropertyRuntimeAction::DynamicMissing {
                    warns_on_miss: false,
                });
            }
            // A `readonly` class carries php's no-dynamic-properties flag, so CREATING the name
            // is an `Error`, never a store. Answering here keeps the write arms' invariant intact:
            // a write may never see `DynamicMissing`, and a readonly class deliberately reserves
            // no hash. A read or an `unset()` needs no arm of its own, because nothing can ever
            // have been created, which is exactly what `DynamicMissing` already means.
            if kind.creates_dynamic_property() && class_is_readonly(ctx, class_name) {
                return Ok(PropertyRuntimeAction::Refuse {
                    message: format!(
                        "Cannot create dynamic property {}::${}",
                        class_name.trim_start_matches('\\'),
                        property
                    ),
                });
            }
            let warns_on_miss = property_name_is_scope_dynamic(ctx, class_name, property);
            Ok(
                match dynamic_property_hash_offset_for_class(ctx, class_name, property)? {
                    Some(hash_offset) => PropertyRuntimeAction::DynamicHash {
                        hash_offset,
                        warns_on_miss,
                    },
                    None => PropertyRuntimeAction::DynamicMissing { warns_on_miss },
                },
            )
        }
        PropertyNameResolution::Inaccessible(visibility) => {
            // A SILENT probe is php's one answer that reports nothing at all: `isset()`, `empty()`
            // and `??` answer false or null without raising. Every other access raises.
            if matches!(
                kind,
                PropertyAccessKind::DirectRead(mode) | PropertyAccessKind::RuntimeRead(mode)
                    if !mode.is_read()
            ) {
                return Ok(PropertyRuntimeAction::DynamicMissing {
                    warns_on_miss: false,
                });
            }
            Ok(PropertyRuntimeAction::Refuse {
                message: property_access_error_message(&visibility, class_name, property),
            })
        }
    }
}

/// Returns whether the class is declared `readonly`, which php makes no-dynamic-properties.
fn class_is_readonly(ctx: &FunctionContext<'_>, class_name: &str) -> bool {
    ctx.module
        .class_infos
        .get(class_name.trim_start_matches('\\'))
        .is_some_and(|class_info| class_info.is_readonly_class)
}

/// Returns whether the class, or an ancestor, declares one magic accessor.
///
/// `ClassInfo::methods` is already flattened over the ancestry, so an inherited accessor counts,
/// exactly as `magic_get_receiver_class` reads it.
pub(super) fn class_declares_method(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method: &str,
) -> bool {
    ctx.module
        .class_infos
        .get(class_name.trim_start_matches('\\'))
        .is_some_and(|class_info| class_info.methods.contains_key(&php_symbol_key(method)))
}

/// How much temporary stack the calling site has reserved above the dispatch.
///
/// A `Refuse` arm raises before it can return, so the block has to be released BEFORE the raise
/// or the unwinder sees a stack pointer no other arm left behind. Sites pass their own number
/// rather than the emitter assuming one, because the ladders reserve different frames: a
/// direct-name site has none, a runtime-name site has its receiver and key staged in 32 bytes.
#[derive(Clone, Copy)]
pub(super) struct DispatchStackCleanup(pub(super) usize);

impl DispatchStackCleanup {
    /// The site holds no temporary block over the dispatch.
    pub(super) const NONE: Self = Self(0);

    /// Releases the site's block, if it reserved one.
    fn release(self, ctx: &mut FunctionContext<'_>) {
        if self.0 > 0 {
            abi::emit_release_temporary_stack(ctx.emitter, self.0);
        }
    }
}

/// Emits one property access, once per runtime class the receiver can hold.
///
/// The emitter owns the two arms whose behaviour does not depend on the operation: `Refuse`
/// releases the site's stack block and raises, and `MagicDeferred` is handed back to the site
/// because only the site knows how to spell php's null in its own result representation.
///
/// `emit_action` sees `Slot`, `DynamicHash`, `DynamicMissing` and `MagicDeferred`, and must leave
/// the machine in the same state on every arm, because the arms converge. A `Refuse` arm never
/// converges: it raises.
pub(super) fn emit_property_runtime_dispatch(
    ctx: &mut FunctionContext<'_>,
    plan: &PropertyRuntimePlan,
    label_prefix: &str,
    cleanup: DispatchStackCleanup,
    mut emit_class_id_probe: impl FnMut(&mut FunctionContext<'_>, u64, &str),
    mut emit_action: impl FnMut(&mut FunctionContext<'_>, &PropertyRuntimeArm) -> Result<()>,
) -> Result<()> {
    if let Some(arm) = plan.fixed_arm() {
        return emit_plan_arm(ctx, arm, cleanup, &mut emit_action);
    }
    let PropertyRuntimePlan::ByClassId(arms) = plan else {
        return Err(CodegenIrError::invalid_module("property plan without arms"));
    };
    let Some((fallthrough, dispatched)) = arms.split_last() else {
        return Err(CodegenIrError::invalid_module("property plan without arms"));
    };
    let labels = dispatched
        .iter()
        .map(|arm| {
            ctx.next_label(&format!(
                "{}_{}",
                label_prefix,
                label_fragment(&arm.class_name)
            ))
        })
        .collect::<Vec<_>>();
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    for (arm, label) in dispatched.iter().zip(labels.iter()) {
        if let Some(class_id) = arm.class_id {
            emit_class_id_probe(ctx, class_id, label);
        }
    }
    // No probe matched, so the receiver is the static class itself or a subclass the module never
    // enumerated, which the `Fixed` proof above is exactly about.
    emit_plan_arm(ctx, fallthrough, cleanup, &mut emit_action)?;
    abi::emit_jump(ctx.emitter, &done_label);
    for (arm, label) in dispatched.iter().zip(labels.iter()) {
        ctx.emitter.label(label);
        emit_plan_arm(ctx, arm, cleanup, &mut emit_action)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits one arm, handling the operation-independent refusal itself.
fn emit_plan_arm(
    ctx: &mut FunctionContext<'_>,
    arm: &PropertyRuntimeArm,
    cleanup: DispatchStackCleanup,
    emit_action: &mut impl FnMut(&mut FunctionContext<'_>, &PropertyRuntimeArm) -> Result<()>,
) -> Result<()> {
    if let PropertyRuntimeAction::Refuse { message } = &arm.action {
        let message = message.clone();
        cleanup.release(ctx);
        super::super::exceptions::emit_error(ctx, &message);
        return Ok(());
    }
    emit_action(ctx, arm)
}

/// Emits one property access for a receiver still held as an SSA value.
///
/// The receiver is materialized ONCE, before any arm, because every probe is emitted ahead of
/// every action body: an action is free to clobber the probe register afterwards.
pub(super) fn emit_object_property_runtime_dispatch(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    plan: &PropertyRuntimePlan,
    label_prefix: &str,
    cleanup: DispatchStackCleanup,
    emit_action: impl FnMut(&mut FunctionContext<'_>, &PropertyRuntimeArm) -> Result<()>,
) -> Result<()> {
    if plan.fixed_arm().is_some() {
        return emit_property_runtime_dispatch(
            ctx,
            plan,
            label_prefix,
            cleanup,
            |_, _, _| {},
            emit_action,
        );
    }
    let probe_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, probe_reg)?;
    emit_property_runtime_dispatch(
        ctx,
        plan,
        label_prefix,
        cleanup,
        |ctx, class_id, label| {
            emit_branch_if_object_reg_class_matches(ctx, probe_reg, class_id, label)
        },
        emit_action,
    )
}

/// Reports a write arm that reached `DynamicMissing`, which the reservation must prevent.
///
/// `crate::types::checker::scope_dynamic_storage` reserves the per-instance hash for exactly the
/// classes a reachable mutation can address, and expands that over subclasses, so a write whose
/// runtime class resolves the name dynamically always has somewhere to put the value. Reaching
/// here means the reservation and the backend disagree, and php stores a value in that program,
/// so the honest answer is to fail the build rather than to drop the write in silence.
pub(super) fn dynamic_write_without_storage(class_name: &str, property: &str) -> CodegenIrError {
    CodegenIrError::invalid_module(format!(
        "write to dynamic property {}::${} reached a class with no per-instance property hash; \
         scope-dynamic storage reservation and backend dispatch disagree",
        class_name, property
    ))
}

/// Compares a receiver held in `object_reg` against one candidate class id.
///
/// The runtime class id is the object's header word, the same one
/// `emit_branch_if_stacked_object_class_matches` reads for the Mixed ladders.
///
/// The receiver SURVIVES the probe, because a ladder emits every probe before any arm body and
/// each probe reads the receiver again. Both scratch registers therefore have to be distinct from
/// `object_reg`, which callers take from `abi::symbol_scratch_reg`: `x9` on AArch64 and `r11` on
/// x86_64. Spelling the candidate register `r11` literally aliased the receiver on x86_64, so the
/// first non-matching probe overwrote it with a class id and the NEXT probe dereferenced that
/// immediate as a pointer. `abi::tertiary_scratch_reg` is `x11` and `rcx`, which keeps AArch64
/// byte-identical and removes the alias on x86_64.
pub(super) fn emit_branch_if_object_reg_class_matches(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    class_id: u64,
    matched_label: &str,
) {
    let header_reg = abi::secondary_scratch_reg(ctx.emitter);
    let candidate_reg = abi::tertiary_scratch_reg(ctx.emitter);
    debug_assert!(
        header_reg != object_reg && candidate_reg != object_reg,
        "class-id probe must not clobber the receiver it reads on every arm"
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr {}, [{}]", header_reg, object_reg)); // load the receiver's runtime class id from its object header
            abi::emit_load_int_immediate(ctx.emitter, candidate_reg, class_id as i64);
            ctx.emitter
                .instruction(&format!("cmp {}, {}", header_reg, candidate_reg)); // compare it with this candidate class
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // take that class's own runtime action
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov {}, QWORD PTR [{}]", header_reg, object_reg)); // load the receiver's runtime class id from its object header
            abi::emit_load_int_immediate(ctx.emitter, candidate_reg, class_id as i64);
            ctx.emitter
                .instruction(&format!("cmp {}, {}", header_reg, candidate_reg)); // compare it with this candidate class
            ctx.emitter.instruction(&format!("je {}", matched_label));          // take that class's own runtime action
        }
    }
}

/// Returns every property NAME a RUNTIME name can match on a receiver of this static class.
///
/// A by-name ladder used to enumerate the static class's layout alone, which is only an upper
/// bound on the receiver: a module-declared subclass can INTRODUCE a property its parent never
/// had, and php answers `$base->{$k}` from that subclass's own slot when the instance really is
/// one. A name missing from the ladder fell into the hash miss arm instead, so the access went to
/// the per-instance hash while php went to a declared slot, and the two disagreed about both the
/// value and the storage.
///
/// The static class's own layout order comes FIRST and is preserved exactly, so a program whose
/// receiver class has no declared subclass emits the same ladder it emitted before. Names only a
/// subclass declares follow, ordered by class id and then by that class's layout, which is
/// deterministic regardless of how `class_infos` iterates.
pub(super) fn runtime_name_candidate_properties(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> Result<Vec<String>> {
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", normalized)))?;
    let mut names = class_info
        .properties
        .iter()
        .map(|(property, _)| property.clone())
        .collect::<Vec<_>>();
    let mut subclasses = ctx
        .module
        .class_infos
        .iter()
        .filter(|(candidate, _)| {
            candidate.as_str() != normalized
                && crate::types::class_inherits_from(
                    &ctx.module.class_infos,
                    candidate.as_str(),
                    normalized,
                )
        })
        .collect::<Vec<_>>();
    subclasses.sort_by_key(|(_, candidate_info)| candidate_info.class_id);
    for (_, candidate_info) in subclasses {
        for (property, _) in &candidate_info.properties {
            if !names.iter().any(|name| name == property) {
                names.push(property.clone());
            }
        }
    }
    Ok(names)
}

/// Returns the per-runtime-class plan for a name php does NOT answer from a declared slot on the
/// receiver's static class, or `None` when this access is an ordinary declared-slot one.
///
/// The entry condition is deliberately the one the hash routes already used: the name is a strict
/// ancestor's private slot in this scope, or the static class answers it from its per-instance
/// hash. Everything else keeps the declared-slot path it had before, so this phase does not
/// change what an ordinary property access emits.
///
/// What the plan adds inside that condition is php's answer per RUNTIME class, which can differ
/// in kind and not only in address: `#[\AllowDynamicProperties] class A {} class B extends A {
/// public $p; }` answers `$a->p` from the hash on an `A` and from `B`'s own slot on a `B`, and the
/// same is true of a subclass that redeclares a strict ancestor's private name as public.
pub(super) fn dynamic_property_runtime_plan_for_object(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    property: &str,
    kind: PropertyAccessKind,
    inst: &Instruction,
) -> Result<Option<PropertyRuntimePlan>> {
    let PhpType::Object(class_name) = ctx.value_php_type(object)? else {
        return Ok(None);
    };
    dynamic_property_runtime_plan_for_class(ctx, &class_name, property, kind, inst)
}

/// The named-class form of [`dynamic_property_runtime_plan_for_object`].
pub(super) fn dynamic_property_runtime_plan_for_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    kind: PropertyAccessKind,
    inst: &Instruction,
) -> Result<Option<PropertyRuntimePlan>> {
    let normalized = class_name.trim_start_matches('\\');
    let answers_dynamically = property_name_is_scope_dynamic(ctx, normalized, property)
        || dynamic_property_hash_offset_for_class(ctx, normalized, property)?.is_some()
        || subtree_answers_from_its_own_hash(ctx, normalized, property)?
        || kind
            .magic_method()
            .is_some_and(|method| subtree_declares_magic_method(ctx, normalized, method));
    if !answers_dynamically {
        return Ok(None);
    }
    resolve_property_runtime_plan(ctx, normalized, property, kind, inst).map(Some)
}

/// Returns whether the receiver class or one of its runtime subclasses declares an accessor.
fn subtree_declares_magic_method(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method: &str,
) -> bool {
    ctx.module.class_infos.iter().any(|(candidate, class_info)| {
        (candidate == class_name
            || crate::types::class_inherits_from(
                &ctx.module.class_infos,
                candidate,
                class_name,
            ))
            && class_info.methods.contains_key(&php_symbol_key(method))
    })
}

/// Returns whether a module-declared SUBCLASS answers this name from its OWN per-instance hash
/// while the static class does not resolve it to a slot this scope can see.
///
/// Asking only the static class made the plan ineligible for the case where the base has no hash
/// at all: `class P {} #[\AllowDynamicProperties] class Q extends P {}` with `f(P $x, $k) { $x->{$k} = 1; }`
/// found no storage on `P`, so the access fell through to a path that DROPPED the write, while php
/// stores it on the `Q` the receiver really is. The static class is only an upper bound on the
/// runtime class, and hash storage is a per-class property, so the eligibility question has to be
/// asked of the whole subtree.
///
/// The guard on the front is what keeps the blast radius where phase B2 put it: a name the static
/// class resolves to a VISIBLE slot keeps its ordinary declared-slot lowering untouched. php
/// forbids weakening visibility in a subclass, so such a name stays reachable on every subclass
/// and keeps the inherited slot index.
fn subtree_answers_from_its_own_hash(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> Result<bool> {
    if matches!(
        resolve_property_name_in_current_scope(ctx, class_name, property),
        PropertyNameResolution::Visible | PropertyNameResolution::ScopePrivate { .. }
    ) {
        return Ok(false);
    }
    for (candidate, _) in &ctx.module.class_infos {
        if candidate.as_str() == class_name
            || !crate::types::class_inherits_from(
                &ctx.module.class_infos,
                candidate.as_str(),
                class_name,
            )
        {
            continue;
        }
        if dynamic_property_hash_offset_for_class(ctx, candidate, property)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Resolves what php does with one property NAME on `class_name` from the body's LEXICAL scope.
///
/// `Function::lexical_class` is the php scope the frame executes in: the declaring class for a
/// method, the declaring class for a closure written inside one, the planned invocation scope for
/// a `clone()` override applicator (`crate::ir_lower::clone_overrides`), and `None` for global
/// scope. It is the same source `clone_hook_is_visible` uses for `__clone` visibility.
///
/// Every by-name ladder over `ClassInfo::properties` has to consult this before it matches a
/// runtime name against a slot: the physical layout still carries a strict ancestor's private
/// slot under its plain name, and php resolves that name to a DYNAMIC property everywhere except
/// inside the class that declared it. See `crate::types::resolve_property_name`.
pub(super) fn resolve_property_name_in_current_scope(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> PropertyNameResolution {
    crate::types::resolve_property_name(
        &ctx.module.class_infos,
        class_name,
        property,
        ctx.function.lexical_class.as_deref(),
    )
}

/// php's answer for ONE property name on ONE class, as the `Mixed` candidate builders read it.
///
/// The arm no longer carries the property NAME. Every caller already knows which name it asked
/// about, and the by-name ladders that once selected an arm by comparing that field are gone:
/// they resolve a per-runtime-class plan per candidate name instead. Keeping a second copy of the
/// name here only made it possible for the two to disagree. `Refuse` still carries php's verbatim
/// message, which names the class and the property, so no diagnostic loses anything.
pub(super) enum PropertyNameArm {
    /// The name addresses this slot.
    Slot(PropertySlot),
    /// php refuses the access from this scope: raise the catchable `Error` carrying this message.
    Refuse {
        /// php 8.5's verbatim wording, e.g. `Cannot access private property D::$n`.
        message: String,
    },
    /// php resolves the name to a DYNAMIC property here, so it answers from the instance hash.
    ///
    /// Only a READ builds this arm. A strict ancestor's private slot still occupies the physical
    /// layout under this plain name, so the arm exists to keep the name away from that slot and
    /// send it to the per-instance hash, or to php `null` when the class reserves no hash.
    ScopeDynamic,
    /// php would answer this name from `__get` or `__isset`, which is not dispatchable yet.
    ///
    /// The class declares the accessor php consults BEFORE it reports anything, so neither the
    /// access `Error` nor the `Undefined property` warning may be emitted here: php reports
    /// neither. This compiler cannot call the accessor with a runtime name yet, so the arm answers
    /// php `null` and, above all, never reads the slot. The slot holds private storage this scope
    /// may not see, and handing it back would be a storage escape dressed up as a value.
    ///
    /// The php-correct value arrives with the dedicated runtime-name magic dispatch phase.
    MagicDeferred,
}

/// Returns php's fetch mode for one property-read instruction, defaulting to a value read.
///
/// A missing immediate means `Read`, the raising and warning variant, so an emitter that forgets
/// the immediate cannot silently downgrade a value read into a silent probe.
pub(super) fn property_fetch_mode(inst: &Instruction) -> PropertyFetchMode {
    match inst.immediate {
        Some(Immediate::PropertyFetchMode(mode)) => mode,
        _ => PropertyFetchMode::Read,
    }
}


/// Resolves the ladder arm one property name takes on a READ, or `None` when it drops out.
///
/// The FETCH MODE is what separates php's two answers for a name this scope may not reach.
/// A value read raises the catchable `Error`; `isset()`, `empty()` and `??` answer `null` in
/// silence, so their arm is dropped and the name falls into the caller's miss path. That is the
/// whole reason `Op::DynamicPropGet` carries `PropertyFetchMode`: without it, refusing here would
/// have made `isset($o->{$k})` throw, and accepting here let an unrelated scope read private
/// storage.
pub(super) fn resolve_property_read_arm(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    mode: PropertyFetchMode,
    inst: &Instruction,
) -> Result<Option<PropertyNameArm>> {
    let normalized = class_name.trim_start_matches('\\');
    let resolution = resolve_property_name_in_current_scope(ctx, normalized, property);
    // php consults the magic accessor BEFORE it reports either diagnostic: a private property
    // reached from an unrelated scope on a class that declares `__get` answers `__get`, not
    // `Cannot access private property`, and `isset()` there answers `__isset`. This compiler does
    // not dispatch magic for a runtime name yet, so neither the refusal nor the undefined-property
    // warning may be emitted for such a class: both would be diagnostics php never reports.
    if class_declares_property_magic(ctx, normalized, mode)
        && !matches!(resolution, PropertyNameResolution::Visible)
    {
        return Ok(Some(PropertyNameArm::MagicDeferred));
    }
    match resolution {
        PropertyNameResolution::Visible => {
            resolve_property_slot_for_class(ctx, normalized, property, inst)
                .map(|slot| Some(PropertyNameArm::Slot(slot)))
        }
        PropertyNameResolution::ScopePrivate { scope, index } => {
            resolve_scope_private_property_slot(ctx, &scope, index, property, inst)
                .map(|slot| Some(PropertyNameArm::Slot(slot)))
        }
        PropertyNameResolution::Dynamic => Ok(Some(PropertyNameArm::ScopeDynamic)),
        PropertyNameResolution::Inaccessible(visibility) => Ok(mode.is_read().then(|| {
            PropertyNameArm::Refuse {
                message: property_access_error_message(&visibility, normalized, property),
            }
        })),
    }
}

/// Returns whether the class declares the magic accessor php would consult in this fetch mode.
///
/// `__get` for a value read, `__isset` for a probe. `ClassInfo::methods` is already flattened over
/// the ancestry, so an inherited accessor counts, exactly as `magic_get_receiver_class` reads it.
fn class_declares_property_magic(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    mode: PropertyFetchMode,
) -> bool {
    let magic = if mode.is_read() { "__get" } else { "__isset" };
    ctx.module
        .class_infos
        .get(class_name)
        .is_some_and(|class_info| class_info.methods.contains_key(&php_symbol_key(magic)))
}

/// Resolves the ladder arm a property name takes for a REFERENCE binding on ONE class.
///
/// `$x = &$o->p`, a by-reference argument and a by-reference return all hand the CALLER the
/// address of the storage, which is the widest exposure a property access has: whatever the cell
/// aliases can be read and written for as long as the alias lives. So the accessibility question
/// is asked here in full, not deferred to whoever dereferences the pointer.
///
/// `Visible` and `ScopePrivate` take the scope-selected slot, exactly as before. `Inaccessible`
/// becomes a refusal carrying php's verbatim message instead of handing back the slot, which is
/// what pre-B1 did and what phase B1 deliberately left standing for this phase. `Dynamic` has no
/// slot at all: php creates a distinct dynamic property and binds the reference to THAT, so the
/// one answer that must never be given is the strict ancestor's physical slot.
///
/// Binding a reference INTO the per-instance hash is a capability this compiler does not have
/// yet, for any class: `$r = &$o->d` on a plain `stdClass` is already an `unsupported` backend
/// diagnostic. `None` therefore surfaces as that same diagnostic, which is a refusal to compile
/// rather than a silent read of storage this scope may not see.
pub(super) fn resolve_property_reference_arm(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    inst: &Instruction,
) -> Result<Option<PropertyNameArm>> {
    let normalized = class_name.trim_start_matches('\\');
    match resolve_property_name_in_current_scope(ctx, normalized, property) {
        PropertyNameResolution::Visible => {
            resolve_property_slot_for_class(ctx, normalized, property, inst)
                .map(|slot| Some(PropertyNameArm::Slot(slot)))
        }
        PropertyNameResolution::ScopePrivate { scope, index } => {
            resolve_scope_private_property_slot(ctx, &scope, index, property, inst)
                .map(|slot| Some(PropertyNameArm::Slot(slot)))
        }
        PropertyNameResolution::Dynamic => Ok(None),
        PropertyNameResolution::Inaccessible(visibility) => Ok(Some(PropertyNameArm::Refuse {
            message: property_access_error_message(&visibility, normalized, property),
        })),
    }
}

/// Formats php 8.5's verbatim member-access refusal for one property.
///
/// Measured against php 8.5.10: `Cannot access private property D::$n` and
/// `Cannot access protected property Prot::$p`, with no scope suffix on either.
fn property_access_error_message(
    visibility: &Visibility,
    class_name: &str,
    property: &str,
) -> String {
    let label = match visibility {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Private => "private",
    };
    format!(
        "Cannot access {} property {}::${}",
        label, class_name, property
    )
}

/// Returns the visibility refusal a suppressed `__set` reentry must preserve for this name.
///
/// A missing or strict-ancestor-private name is dynamic and may be stored after PHP suppresses a
/// recursive receiver/name pair. A private or protected slot owned by the runtime class remains
/// inaccessible, so suppression must raise the original access error instead of turning the name
/// into a public dynamic hash entry.
pub(super) fn magic_set_recursive_refusal(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> Option<String> {
    match resolve_property_name_in_current_scope(ctx, class_name, property) {
        PropertyNameResolution::Inaccessible(visibility) => Some(
            property_access_error_message(&visibility, class_name, property),
        ),
        _ => None,
    }
}

/// Builds the slot metadata for a private property the INVOCATION SCOPE declares.
///
/// The offset is computed from the scope class's own layout index, which is also the receiver's:
/// `ClassBuildState::inherit_properties` pushes a parent's slots first and in order, so a
/// subclass layout starts with an exact copy of its parent's prefix.
fn resolve_scope_private_property_slot(
    ctx: &FunctionContext<'_>,
    scope_class: &str,
    index: usize,
    property: &str,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let class_info = ctx
        .module
        .class_infos
        .get(scope_class)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", scope_class)))?;
    let (slot_property, php_type) = class_info.properties.get(index).ok_or_else(|| {
        CodegenIrError::invalid_module("scope-private property index is outside the class layout")
    })?;
    let php_type = runtime_property_type_override(ctx, scope_class, slot_property)
        .unwrap_or_else(|| php_type.clone());
    ensure_property_type_supported(&php_type, inst)?;
    Ok(PropertySlot {
        class_name: scope_class.to_string(),
        property: property.to_string(),
        php_type,
        offset: 8 + index * 16,
        is_declared: class_info.property_slot_is_declared(index, slot_property),
        is_packed: false,
        is_reference: class_info.property_slot_is_reference(index, slot_property),
    })
}

/// Returns whether the physical layout carries `property` but php resolves it to a DYNAMIC
/// property in this scope.
///
/// The physical-slot test is what keeps this narrow. `resolve_property_name` answers `Dynamic`
/// for every name a class does not declare, so without it an ordinary undeclared name on an
/// `#[\AllowDynamicProperties]` class would take the scope-dynamic route as well. Only a strict
/// ancestor's private slot is both present in the layout and invisible by name.
pub(super) fn property_name_is_scope_dynamic(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> bool {
    crate::types::property_name_shadows_ancestor_private_slot(
        &ctx.module.class_infos,
        class_name,
        property,
        ctx.function.lexical_class.as_deref(),
    )
}

/// Returns the receiver's class when `property` is a scope-dynamic name on it.
pub(super) fn scope_dynamic_property_class_for_object(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    property: &str,
) -> Result<Option<String>> {
    let PhpType::Object(class_name) = ctx.value_php_type(object)? else {
        return Ok(None);
    };
    let normalized = class_name.trim_start_matches('\\').to_string();
    Ok(property_name_is_scope_dynamic(ctx, &normalized, property).then_some(normalized))
}

/// Returns the dynamic-property hash slot offset for a known class and property name.
pub(super) fn dynamic_property_hash_offset_for_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> Result<Option<usize>> {
    let normalized = class_name.trim_start_matches('\\');
    if is_builtin_stdclass(normalized) {
        return Ok(Some(dynamic_property_hash_offset(0)));
    }
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", normalized)))?;
    // A slot this SCOPE does not resolve the name to is not a collision: php keeps a strict
    // ancestor's private property under a mangled key, so the plain name belongs to the hash here.
    if class_info
        .properties
        .iter()
        .any(|(name, _)| name == property)
        && resolve_property_name_in_current_scope(ctx, normalized, property)
            != PropertyNameResolution::Dynamic
    {
        return Ok(None);
    }
    if class_info.dynamic_property_hash_is_name_addressable() {
        return Ok(Some(dynamic_property_hash_offset(
            class_info.properties.len(),
        )));
    }
    Ok(None)
}

/// Returns true when a class name is the builtin `stdClass` dynamic-property container.
pub(super) fn is_builtin_stdclass(class_name: &str) -> bool {
    crate::types::checker::builtin_stdclass::is_stdclass(class_name.trim_start_matches('\\'))
}

/// Returns true when the SSA value is known to hold a stdClass object pointer.
pub(super) fn object_is_builtin_stdclass(ctx: &FunctionContext<'_>, object: ValueId) -> Result<bool> {
    Ok(matches!(
        ctx.value_php_type(object)?.codegen_repr(),
        PhpType::Object(class_name) if is_builtin_stdclass(&class_name)
    ))
}

/// Resolves a property slot for a known class name.
pub(super) fn resolve_property_slot_for_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", normalized)))?;
    let Some((index, (_, php_type))) = class_info.visible_property(property) else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for dynamic or missing property {}::${}",
            inst.op.name(),
            normalized,
            property
        )));
    };
    let is_reference = class_info.property_slot_is_reference(index, property);
    let php_type = runtime_property_type_override(ctx, normalized, property)
        .unwrap_or_else(|| php_type.clone());
    ensure_property_type_supported(&php_type, inst)?;
    let offset = 8 + index * 16;
    Ok(PropertySlot {
        class_name: normalized.to_string(),
        property: property.to_string(),
        php_type,
        offset,
        is_declared: class_info.property_slot_is_declared(index, property),
        is_packed: false,
        is_reference,
    })
}

/// Returns precise runtime storage types for inherited SPL callback-filter internals.
pub(super) fn runtime_property_type_override(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> Option<PhpType> {
    if !class_extends_class(ctx, class_name, "CallbackFilterIterator") {
        return None;
    }
    match property {
        "callback" => Some(PhpType::Callable),
        "callbackEnv" => Some(PhpType::Pointer(None)),
        _ => None,
    }
}

/// Returns the source PHP type for an SSA value before codegen representation erasure.
pub(in crate::codegen::lower_inst) fn raw_value_php_type(ctx: &FunctionContext<'_>, value: ValueId) -> Result<PhpType> {
    ctx.function
        .value(value)
        .map(|metadata| metadata.php_type.clone())
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))
}

/// Returns the literal string payload for a value produced by `ConstStr`, when statically known.
pub(super) fn const_string_operand<'a>(ctx: &FunctionContext<'a>, value: ValueId) -> Result<Option<&'a str>> {
    let metadata = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = metadata.def else {
        return Ok(None);
    };
    let instruction = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if instruction.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = instruction.immediate else {
        return Err(CodegenIrError::invalid_module(
            "const_str missing data immediate",
        ));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .map(Some)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Resolves an object or object|null source type for a nullsafe receiver.
pub(in crate::codegen::lower_inst) fn nullable_object_receiver_class(
    ctx: &FunctionContext<'_>,
    object: ValueId,
) -> Result<Option<(String, bool)>> {
    match raw_value_php_type(ctx, object)? {
        PhpType::Object(class_name) => Ok(Some((class_name, false))),
        PhpType::Union(members) => {
            let mut class_name = None;
            let mut nullable = false;
            for member in members {
                match member {
                    PhpType::Void => nullable = true,
                    PhpType::Object(candidate) => {
                        if class_name
                            .as_ref()
                            .is_some_and(|existing: &String| existing != &candidate)
                        {
                            return Ok(None);
                        }
                        class_name = Some(candidate);
                    }
                    _ => return Ok(None),
                }
            }
            Ok(class_name.map(|name| (name, nullable)))
        }
        _ => Ok(None),
    }
}

/// Returns the unique object class carried by a boxed union, ignoring null and scalar arms.
pub(super) fn union_object_member_class(ctx: &FunctionContext<'_>, object: ValueId) -> Result<Option<String>> {
    let PhpType::Union(members) = raw_value_php_type(ctx, object)? else {
        return Ok(None);
    };
    let mut class_name = None;
    for member in members {
        let PhpType::Object(candidate) = member else {
            continue;
        };
        if class_name
            .as_ref()
            .is_some_and(|existing: &String| existing != &candidate)
        {
            return Ok(None);
        }
        class_name = Some(candidate);
    }
    Ok(class_name)
}

/// Unboxes a nullable object receiver and branches when it holds PHP null.
pub(in crate::codegen::lower_inst) fn emit_nullable_receiver_object_payload(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    null_label: &str,
    object_reg: &str,
) -> Result<()> {
    let ty = ctx.load_value_to_result(object)?;
    if ty != PhpType::Mixed {
        return Err(CodegenIrError::unsupported(format!(
            "nullsafe property receiver storage {:?}",
            ty
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #8");                              // check whether the nullable receiver holds PHP null
            ctx.emitter.instruction(&format!("b.eq {}", null_label));           // short-circuit property access for nullsafe null receivers
            ctx.emitter.instruction(&format!("mov {}, x1", object_reg));        // promote the unboxed object payload into the property base register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 8");                              // check whether the nullable receiver holds PHP null
            ctx.emitter.instruction(&format!("je {}", null_label));             // short-circuit property access for nullsafe null receivers
            ctx.emitter.instruction(&format!("mov {}, rdi", object_reg));       // promote the unboxed object payload into the property base register
        }
    }
    Ok(())
}

/// Boxes a PHP null sentinel as a runtime Mixed cell.
pub(in crate::codegen::lower_inst) fn emit_boxed_null(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        RUNTIME_NULL_SENTINEL,
    );
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}

/// Resolves a field slot on an embedded packed-class receiver.
pub(super) fn resolve_packed_field_slot(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .packed_class_infos
        .get(normalized)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!("unknown packed class {}", normalized))
        })?;
    let Some(field) = class_info
        .fields
        .iter()
        .find(|field| field.name == property)
    else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for missing packed field {}::${}",
            inst.op.name(),
            normalized,
            property
        )));
    };
    ensure_property_type_supported(&field.php_type, inst)?;
    Ok(PropertySlot {
        class_name: normalized.to_string(),
        property: property.to_string(),
        php_type: field.php_type.clone(),
        offset: field.offset,
        is_declared: false,
        is_packed: true,
        is_reference: false,
    })
}
