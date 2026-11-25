//! Purpose:
//! Shares repeated literal-name property writes from boxed Mixed receivers.
//!
//! Called from:
//! - Module emission and the existing named and literal dynamic write paths.
//!
//! Key details:
//! - Scope, opcode, exact spelling and the original value type identify one body.
//! - Borrowed, persistent and Mixed values are shared. An owned Mixed box the inline store would
//!   adopt (no EIR release, no scoped cleanup) gets its own body that adopts it the same way, on
//!   every path; other owned refcounted values stay inline.
//! - The inner write carries a compiler-generated span, so the caller's published site survives.
//! - Each call passes its source line for dynamic-property diagnostics.

use std::collections::HashMap;
use super::{abi, block_emit, context::FunctionContext, data_section::DataSection,
    emit::Emitter, shared_state::SharedCodegenState, Result};
use crate::ir::{Builder, DataId, Function, FunctionParam, Immediate, InstId, Instruction,
    IrHeapKind, IrType, LocalKind, LocalSlotId, Module, Op, Ownership, Terminator, ValueDef, ValueId};
use crate::types::PhpType;

const PREFIX: &str = "_eir_shared_literal_property_write_";

#[derive(Clone, PartialEq, Eq, Hash)]
struct WriteKey {
    scope: Option<String>,
    property: String,
    dynamic: bool,
    value_type: PhpType,
    value_ir: IrType,
    /// The site's owned Mixed box is adopted by the store, so the body owns its operand.
    adopt: bool,
}

/// Immutable helper inventory for one module emission.
pub(super) struct WritePlan {
    helpers: Vec<(WriteKey, DataId, String)>,
    labels: HashMap<WriteKey, String>,
}

/// Selects only already evaluated literal writes with a supported borrowed argument ABI.
fn literal_site(function: &Function, inst: &Instruction) -> Option<(DataId, ValueId)> {
    if function.name.starts_with(PREFIX)
        || function.name.contains("::__propget_") || function.name.contains("::__propset_")
        || !function.value(*inst.operands.first()?).is_some_and(|value| value.php_type == PhpType::Mixed)
    { return None; }
    let (data, value) = match inst.op {
        Op::PropSet => match inst.immediate {
            Some(Immediate::Data(data)) => (data, *inst.operands.get(1)?),
            _ => return None,
        },
        Op::DynamicPropSet => {
            let name = function.value(*inst.operands.get(1)?)?;
            let ValueDef::Instruction { inst: name_inst, .. } = name.def else { return None; };
            let literal = function.instruction(name_inst)?;
            if literal.op != Op::ConstStr { return None; }
            let Some(Immediate::Data(data)) = literal.immediate else { return None; };
            (data, *inst.operands.get(2)?)
        }
        _ => return None,
    };
    let operand = function.value(value)?;
    // A borrowed body retains its own reference, and an adopting body takes an owned Mixed box
    // over (see `adopts_owned_box`); other owned refcounted values keep the inline store, which
    // decides their transfer itself.
    if Ownership::php_type_needs_lifetime_tracking(&operand.php_type)
        && !matches!(operand.ownership, Ownership::Borrowed | Ownership::Persistent)
        && operand.php_type.codegen_repr() != PhpType::Mixed
    {
        return None;
    }
    if !matches!(operand.ir_type, IrType::I64 | IrType::Str | IrType::Heap(_))
        || !matches!(operand.php_type.codegen_repr(), PhpType::Int | PhpType::Bool | PhpType::False
            | PhpType::Str | PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_))
    { return None; }
    Some((data, value))
}

/// Returns whether the inline store would adopt this site's owned Mixed box.
///
/// This is `FunctionContext::value_can_own_mixed_box_source` for a Mixed value: an owned box with
/// no EIR `Release` and no scoped call-operand cleanup is EIR's promise that the store takes it
/// over. `prepare` and `helper_label` both decide through this one predicate.
fn adopts_owned_box(function: &Function, value: ValueId, inst: InstId) -> bool {
    function.value(value).is_some_and(|operand| {
        operand.php_type.codegen_repr() == PhpType::Mixed && operand.ownership == Ownership::Owned
    }) && !super::context::operand_owners::has_scoped_cleanup(function, value, Some(inst))
        && !function.instructions.iter().any(|candidate| {
            candidate.op == Op::Release && candidate.operands.first() == Some(&value)
        })
}

/// Counts sites once while keeping visibility, type checks and opcode-specific precedence separate.
fn prepare(module: &Module, shared: &mut SharedCodegenState) {
    if shared.literal_property_write_helpers.is_some() { return; }
    let mut counts: HashMap<WriteKey, (usize, DataId)> = HashMap::new();
    for function in module.functions.iter().chain(&module.class_methods).chain(&module.closures) {
        for (index, inst) in function.instructions.iter().enumerate() {
            let Some((data, value)) = literal_site(function, inst) else { continue; };
            let Some(property) = module.data.strings.get(data.as_raw() as usize) else { continue; };
            let operand = function.value(value).expect("eligible write value exists");
            let scope = function.inlined_scopes.get(&InstId::from_raw(index as u32))
                .cloned().or_else(|| function.lexical_class.clone());
            let adopt = adopts_owned_box(function, value, InstId::from_raw(index as u32));
            let key = WriteKey { scope, property: property.clone(), dynamic: inst.op == Op::DynamicPropSet,
                value_type: operand.php_type.clone(), value_ir: operand.ir_type, adopt };
            counts.entry(key).or_insert((0, data)).0 += 1;
        }
    }
    let mut entries = counts.into_iter().filter_map(|(key, (count, data))|
        (count >= 2).then_some((key, data))).collect::<Vec<_>>();
    entries.sort_by(|(a, _), (b, _)| a.scope.cmp(&b.scope).then_with(|| a.property.cmp(&b.property))
        .then_with(|| a.dynamic.cmp(&b.dynamic)).then_with(|| a.adopt.cmp(&b.adopt))
        .then_with(|| format!("{:?}/{:?}", a.value_type, a.value_ir)
            .cmp(&format!("{:?}/{:?}", b.value_type, b.value_ir))));
    let helpers = entries.into_iter().enumerate().map(|(index, (key, data))|
        (key, data, format!("{PREFIX}{index}"))).collect::<Vec<_>>();
    let labels = helpers.iter().map(|(key, _, label)| (key.clone(), label.clone())).collect();
    shared.literal_property_write_helpers = Some(WritePlan { helpers, labels });
}

/// Finds the exact source operation without changing its value contract or lexical scope.
pub(super) fn helper_label(ctx: &mut FunctionContext<'_>, inst: &Instruction,
    property: &str) -> Option<String> {
    let (_, value) = literal_site(ctx.function, inst)?;
    let operand = ctx.function.value(value)?;
    let adopt = adopts_owned_box(ctx.function, value, ctx.current_inst()?);
    let key = WriteKey { scope: ctx.lexical_class().map(str::to_string), property: property.into(),
        dynamic: inst.op == Op::DynamicPropSet, value_type: operand.php_type.clone(), value_ir: operand.ir_type,
        adopt };
    prepare(ctx.module, ctx.shared);
    ctx.shared.literal_property_write_helpers.as_ref()?.labels.get(&key).cloned()
}

/// Borrows the evaluated operands and passes the original diagnostic line as a scalar argument.
pub(super) fn emit_call(ctx: &mut FunctionContext<'_>, object: ValueId, value: ValueId,
    inst: &Instruction, label: &str) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, result)?;
    abi::emit_push_reg(ctx.emitter, result);
    let string_value = ctx.function.value(value).is_some_and(|operand| operand.ir_type == IrType::Str);
    if string_value {
        let (ptr, len) = abi::string_result_regs(ctx.emitter);
        ctx.load_string_value_to_regs(value, ptr, len)?;
        abi::emit_push_reg_pair(ctx.emitter, ptr, len);
    } else {
        ctx.load_value_to_reg(value, result)?;
        abi::emit_push_reg(ctx.emitter, result);
    }
    let target = ctx.emitter.target;
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 0), 16);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), 0);
    if string_value {
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 2), 8);
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, if string_value { 3 } else { 2 }),
        i64::from(inst.span.map_or(0, |span| span.line)));
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    // An adopted box transfers into the helper, whose store takes it over exactly as the inline
    // store would, including when the store throws; the caller keeps no cleanup for it.
    abi::emit_call_label(ctx.emitter, label);
    Ok(())
}

/// Loads the caller's captured diagnostic line without consulting globals changed by a callback.
pub(super) fn emit_source_line(ctx: &mut FunctionContext<'_>, register: &str, fallback: u32) {
    if ctx.function.name.starts_with(PREFIX) {
        let offset = ctx.local_offset(LocalSlotId::from_raw(2)).expect("write helper has a source-line parameter");
        abi::load_at_offset(ctx.emitter, register, offset);
    } else {
        abi::emit_load_int_immediate(ctx.emitter, register, i64::from(fallback));
    }
}

/// Builds an ordinary EIR frame around exactly the original write opcode and value type.
fn helper_function(key: &WriteKey, data: DataId, label: &str) -> Function {
    let mixed = IrType::Heap(IrHeapKind::Mixed);
    let mut function = Function::new(label.into(), IrType::Void, PhpType::Void);
    function.flags.is_synthetic = true;
    function.lexical_class = key.scope.clone();
    for (name, ir_type, php_type) in [("object", mixed, PhpType::Mixed),
        ("value", key.value_ir, key.value_type.clone()), ("line", IrType::I64, PhpType::Int)] {
        function.params.push(FunctionParam { name: name.into(), ir_type, php_type: php_type.clone(),
            by_ref: false, variadic: false });
        let slot = function.add_local(Some(name.into()), ir_type, php_type, LocalKind::BorrowedTemp);
        function.no_epilogue_cleanup_slots.insert(slot);
    }
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let object = builder.emit_load_local(LocalSlotId::from_raw(0), mixed, PhpType::Mixed);
    let value = builder.emit_load_local(LocalSlotId::from_raw(1), key.value_ir, key.value_type.clone());
    builder.set_value_ownership(object, Ownership::Borrowed);
    let value_ownership = if key.adopt {
        Ownership::Owned
    } else if Ownership::php_type_needs_lifetime_tracking(&key.value_type) {
        Ownership::Borrowed
    } else {
        Ownership::NonHeap
    };
    builder.set_value_ownership(value, value_ownership);
    let _ = builder.emit_load_local(LocalSlotId::from_raw(2), IrType::I64, PhpType::Int);
    // The caller's own instruction already published its source line and backtrace site. A
    // compiler-generated span keeps this inner write from republishing line 0 over them.
    let span = Some(crate::span::Span::dummy());
    if key.dynamic {
        let name = builder.emit_const_str(data);
        let _ = builder.emit_with_effects(Op::DynamicPropSet, vec![object, name, value], None,
            IrType::Void, PhpType::Void, Ownership::NonHeap, Op::DynamicPropSet.default_effects(), span);
    } else {
        let _ = builder.emit_with_effects(Op::PropSet, vec![object, value], Some(Immediate::Data(data)),
            IrType::Void, PhpType::Void, Ownership::NonHeap, Op::PropSet.default_effects(), span);
    }
    builder.terminate(Terminator::Return { value: None });
    function
}

/// Emits only repeated semantic shapes, retaining normal frame and exception handling.
pub(super) fn emit_helpers(module: &Module, emitter: &mut Emitter, data: &mut DataSection,
    shared: &mut SharedCodegenState) -> Result<()> {
    prepare(module, shared);
    let helpers = shared.literal_property_write_helpers.as_ref().unwrap().helpers.clone();
    for (key, property, label) in helpers {
        let function = helper_function(&key, property, &label);
        crate::ir::validate_function(&function).map_err(|error|
            super::CodegenIrError::invalid_module(format!("shared literal property write: {error:?}")))?;
        emitter.comment(&format!("shared literal property write: scope={} property={:?} dynamic={} value={:?} adopt={}",
            key.scope.as_deref().unwrap_or("global"), key.property, key.dynamic, key.value_type, key.adopt));
        block_emit::emit_synthetic_function_with_label(module, &function, &label, emitter, data, shared, false)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Builds repeated source sites while allowing scope, opcode and value representation to differ.
    #[allow(clippy::too_many_arguments)]
    fn add_sites(module: &mut Module, scope: Option<&str>, property: &str,
        dynamic: bool, php_type: PhpType, ir_type: IrType, count: usize, adopt: bool) {
        let data = module.data.intern_string(property);
        let key = WriteKey { scope: scope.map(str::to_string), property: property.into(),
            dynamic, value_type: php_type, value_ir: ir_type, adopt };
        for _ in 0..count {
            let name = format!("write_site_{}", module.functions.len());
            module.functions.push(helper_function(&key, data, &name));
        }
    }

    /// Every target retains exact types, visibility scope, borrowed owners and the original opcode.
    fn verify(target: &str) {
        let mut module = Module::new(Target::parse(target).unwrap());
        let mixed = IrType::Heap(IrHeapKind::Mixed);
        add_sites(&mut module, None, "value", false, PhpType::Mixed, mixed, 2, false);
        add_sites(&mut module, None, "value", false, PhpType::Str, IrType::Str, 2, false);
        add_sites(&mut module, None, "value", true, PhpType::Mixed, mixed, 2, false);
        add_sites(&mut module, None, "value", false, PhpType::Array(Box::new(PhpType::Mixed)), IrType::Heap(IrHeapKind::Array), 2, false);
        add_sites(&mut module, Some("Vault"), "value", false, PhpType::Mixed, mixed, 2, false);
        add_sites(&mut module, None, "once", false, PhpType::Int, IrType::I64, 1, false);
        add_sites(&mut module, None, "float", false, PhpType::Float, IrType::F64, 2, false);
        add_sites(&mut module, None, "value", true, PhpType::Mixed, mixed, 2, true);
        let mut shared = SharedCodegenState::default();
        prepare(&module, &mut shared);
        let plan = shared.literal_property_write_helpers.as_ref().unwrap();
        assert_eq!(plan.helpers.len(), 6, "{target}: scope, opcode, value types and adoption stay separate");
        assert_eq!(plan.labels.len(), 6);
        for (key, property, label) in &plan.helpers {
            let function = helper_function(key, *property, label);
            crate::ir::validate_function(&function).unwrap();
            assert_eq!(function.params.len(), 3);
            assert_eq!(function.params[1].php_type, key.value_type);
            assert_eq!(function.params[1].ir_type, key.value_ir);
            assert_eq!(function.params[2].php_type, PhpType::Int);
            assert_eq!(function.no_epilogue_cleanup_slots.len(), 3);
            let opcode = if key.dynamic { Op::DynamicPropSet } else { Op::PropSet };
            let inst = function.instructions.iter().find(|inst| inst.op == opcode).unwrap();
            assert!(literal_site(&function, inst).is_none(), "{target}: helper cannot recurse");
            assert_eq!(function.value(inst.operands[0]).unwrap().ownership, Ownership::Borrowed);
            let value = *inst.operands.last().unwrap();
            assert_eq!(function.value(value).unwrap().php_type, key.value_type);
            let expected = if key.adopt { Ownership::Owned } else { Ownership::Borrowed };
            assert_eq!(function.value(value).unwrap().ownership, expected, "{target}: adopt={}", key.adopt);
        }
        let mut main = Function::new("main".into(), IrType::Void, PhpType::Void);
        let mut builder = Builder::new(&mut main);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Return { value: None });
        module.functions.push(main);
        crate::ir::validate_module(&module).unwrap();
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
        assert!(asm.contains(PREFIX), "{target}: shared write bodies must be emitted and called");
        assert_eq!(asm.matches("shared literal property write:").count(), 6);
        assert_eq!(asm.matches("adopt=true").count(), 1, "{target}: one adopting body");
        // Repeating an independent module emission must not keep another module's helper keys.
        let mut second = Module::new(Target::parse(target).unwrap());
        add_sites(&mut second, None, "different", false, PhpType::Mixed, mixed, 2, false);
        let mut second_state = SharedCodegenState::default();
        prepare(&second, &mut second_state);
        let plan = second_state.literal_property_write_helpers.as_ref().unwrap();
        assert_eq!(plan.helpers.len(), 1);
        assert_eq!(plan.helpers[0].0.property, "different");
    }

    /// Borrowed values and owned Mixed boxes share the helper, other owned refcounted values stay
    /// on the inline store, and the helper's inner store never republishes a source location.
    /// Only an owned Mixed box without EIR cleanup selects the adopting body.
    #[test]
    fn owned_non_mixed_values_stay_inline_and_inner_stores_keep_the_caller_site() {
        let mut module = Module::new(Target::parse("linux-x86_64").unwrap());
        let data = module.data.intern_string("value");
        for dynamic in [false, true] {
            for (value_type, value_ir, owned_shared) in [
                (PhpType::Mixed, IrType::Heap(IrHeapKind::Mixed), true),
                (PhpType::Str, IrType::Str, false),
                (PhpType::Array(Box::new(PhpType::Mixed)), IrType::Heap(IrHeapKind::Array), false),
            ] {
                let key = WriteKey { scope: None, property: "value".into(), dynamic,
                    value_type: value_type.clone(), value_ir, adopt: false };
                let mut function = helper_function(&key, data, &format!("write_site_{dynamic}"));
                let opcode = if dynamic { Op::DynamicPropSet } else { Op::PropSet };
                let index = function.instructions.iter().position(|inst| inst.op == opcode).unwrap();
                let inst = function.instructions[index].clone();
                let inst_id = InstId::from_raw(index as u32);
                assert!(inst.span.is_some_and(|span| !span.is_from_source()), "{inst:?}");
                assert!(literal_site(&function, &inst).is_some(), "a borrowed {value_type:?} is shared");
                let value = *inst.operands.last().unwrap();
                assert!(!adopts_owned_box(&function, value, inst_id), "a borrowed {value_type:?} is retained");
                for ownership in [Ownership::Owned, Ownership::MaybeOwned, Ownership::Moved] {
                    function.values[value.as_raw() as usize].ownership = ownership;
                    assert_eq!(literal_site(&function, &inst).is_some(), owned_shared,
                        "{ownership:?} {value_type:?}");
                    // Only an owned Mixed box without EIR cleanup is left for the store to adopt.
                    assert_eq!(adopts_owned_box(&function, value, inst_id),
                        owned_shared && ownership == Ownership::Owned, "{ownership:?} {value_type:?}");
                }
                function.values[value.as_raw() as usize].ownership = Ownership::Owned;
                let mut release = inst.clone();
                release.op = Op::Release;
                release.operands = vec![value];
                function.instructions.push(release);
                assert!(!adopts_owned_box(&function, value, inst_id), "an EIR release keeps the retain body");
            }
        }
    }

    /// Shared literal write helpers keep their keys and emit on Linux x86_64.
    #[test]
    fn linux_x86_64() {
        verify("linux-x86_64");
    }

    /// Shared literal write helpers keep their keys and emit on Linux AArch64.
    #[test]
    fn linux_aarch64() {
        verify("linux-aarch64");
    }

    /// Shared literal write helpers keep their keys and emit on macOS AArch64.
    #[test]
    fn macos_aarch64() {
        verify("macos-aarch64");
    }

    /// Shared literal write helpers keep their keys and emit on iOS devices.
    #[test]
    fn ios_arm64() {
        verify("ios-arm64");
    }

    /// Shared literal write helpers keep their keys and emit on the iOS Simulator.
    #[test]
    fn ios_sim_arm64() {
        verify("ios-sim-arm64");
    }
}
