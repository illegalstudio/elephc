//! Purpose:
//! Shares repeated literal-name property reads from boxed Mixed receivers.
//!
//! Called from:
//! - Module emission and the ordinary Mixed property-read lowering path.
//!
//! Key details:
//! - Scope, exact property spelling and fetch mode identify one dispatch body.
//! - A borrowed receiver produces the original owned Mixed result through normal EIR cleanup.
//! - Single sites, hook bodies, quiet reads and non-Mixed representations stay inline.
//! - The inner read carries a compiler-generated span, so the caller's published site survives.

use std::collections::HashMap;

use super::{abi, block_emit, context::FunctionContext, data_section::DataSection,
    emit::Emitter, shared_state::SharedCodegenState, Result};
use crate::ir::{Builder, DataId, Function, FunctionParam, Immediate, InstId, Instruction,
    IrHeapKind, IrType, LocalKind, Module, Op, Ownership, PropertyFetchMode, Terminator,
    ValueDef, ValueId};
use crate::types::PhpType;

const PREFIX: &str = "_eir_shared_literal_property_read_";

/// One scope-sensitive property operation, independent of its literal-pool position.
#[derive(Clone, PartialEq, Eq, Hash)]
struct ReadKey {
    scope: Option<String>,
    property: String,
    mode: PropertyFetchMode,
}

/// Immutable helper inventory owned by one module emission.
pub(super) struct ReadPlan {
    helpers: Vec<(ReadKey, DataId, String)>,
    labels: HashMap<ReadKey, String>,
}

/// Returns the original ordinary-read default when no explicit fetch mode exists.
fn fetch_mode(inst: &Instruction) -> PropertyFetchMode {
    match inst.immediate {
        Some(Immediate::PropertyFetchMode(mode)) => mode,
        _ => PropertyFetchMode::Read,
    }
}

/// Finds only literal value reads with the helper's borrowed-input and owned-result ABI.
fn literal_site(function: &Function, inst: &Instruction) -> Option<DataId> {
    if function.name.starts_with(PREFIX)
        || function.name.contains("::__propget_") || function.name.contains("::__propset_")
        || fetch_mode(inst) == PropertyFetchMode::QuietRead
        || inst.result_php_type.codegen_repr() != PhpType::Mixed
        || inst.result_ownership != Ownership::Owned
        || !function.value(*inst.operands.first()?).is_some_and(|value|
            value.php_type.codegen_repr() == PhpType::Mixed)
    { return None; }
    let literal = match inst.op {
        Op::PropGet => inst,
        Op::DynamicPropGet => {
            let name = function.value(*inst.operands.get(1)?)?;
            let ValueDef::Instruction { inst, .. } = name.def else { return None; };
            let literal = function.instruction(inst)?;
            if literal.op != Op::ConstStr { return None; }
            literal
        }
        _ => return None,
    };
    match literal.immediate { Some(Immediate::Data(data)) => Some(data), _ => None }
}

/// Counts eligible source sites once and emits helpers in deterministic semantic-key order.
fn prepare(module: &Module, shared: &mut SharedCodegenState) {
    if shared.literal_property_read_helpers.is_some() { return; }
    let mut counts: HashMap<ReadKey, (usize, DataId)> = HashMap::new();
    for function in module.functions.iter().chain(&module.class_methods).chain(&module.closures) {
        for (index, inst) in function.instructions.iter().enumerate() {
            let Some(data) = literal_site(function, inst) else { continue; };
            let Some(property) = module.data.strings.get(data.as_raw() as usize) else { continue; };
            let scope = function.inlined_scopes.get(&InstId::from_raw(index as u32))
                .cloned().or_else(|| function.lexical_class.clone());
            let key = ReadKey { scope, property: property.clone(), mode: fetch_mode(inst) };
            counts.entry(key).or_insert((0, data)).0 += 1;
        }
    }
    let mut entries = counts.into_iter().filter_map(|(key, (count, data))|
        (count >= 2).then_some((key, data))).collect::<Vec<_>>();
    entries.sort_by(|(left, _), (right, _)| left.scope.cmp(&right.scope)
        .then_with(|| left.property.cmp(&right.property))
        .then_with(|| left.mode.as_eir().cmp(right.mode.as_eir())));
    let helpers = entries.into_iter().enumerate().map(|(index, (key, data))|
        (key, data, format!("{PREFIX}{index}"))).collect::<Vec<_>>();
    let labels = helpers.iter().map(|(key, _, label)| (key.clone(), label.clone())).collect();
    shared.literal_property_read_helpers = Some(ReadPlan { helpers, labels });
}

/// Reuses exactly the original literal site's lexical scope, spelling and fetch semantics.
pub(super) fn helper_label(ctx: &mut FunctionContext<'_>, inst: &Instruction,
    property: &str) -> Option<String> {
    literal_site(ctx.function, inst)?;
    let key = ReadKey { scope: ctx.lexical_class().map(str::to_string),
        property: property.to_string(), mode: fetch_mode(inst) };
    prepare(ctx.module, ctx.shared);
    ctx.shared.literal_property_read_helpers.as_ref()?.labels.get(&key).cloned()
}

/// Passes the already evaluated boxed receiver without transferring or duplicating its owner.
pub(super) fn emit_call(ctx: &mut FunctionContext<'_>, object: ValueId, label: &str) -> Result<()> {
    ctx.load_value_to_reg(object, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    abi::emit_call_label(ctx.emitter, label);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Creates one valid literal property read with an independently selectable source scope.
    fn site(module: &mut Module, name: &str, scope: Option<&str>, property: &str,
        dynamic: bool, mode: PropertyFetchMode) -> (Function, InstId) {
        let data = module.data.intern_string(property);
        let key = ReadKey { scope: scope.map(str::to_string), property: property.to_string(), mode };
        let mut function = helper_function(&key, data, name);
        let index = function.instructions.iter().position(|inst| inst.op == Op::DynamicPropGet).unwrap();
        if !dynamic {
            let inst = &mut function.instructions[index];
            inst.op = Op::PropGet;
            inst.operands.truncate(1);
            inst.immediate = Some(Immediate::Data(data));
        }
        (function, InstId::from_raw(index as u32))
    }

    /// Mixed literals share across opcode forms while exact case, scope and modes stay separate.
    #[test]
    fn literal_reads_partition_scope_spelling_and_mode_on_every_target() {
        for target in ["linux-x86_64", "linux-aarch64", "macos-aarch64", "ios-arm64", "ios-sim-arm64"] {
            let mut module = Module::new(Target::parse(target).unwrap());
            for (index, (scope, property, mode)) in [
                (None, "value", PropertyFetchMode::Read),
                (Some("Vault"), "value", PropertyFetchMode::Read),
                (None, "Value", PropertyFetchMode::Read),
                (None, "value", PropertyFetchMode::Isset),
            ].into_iter().enumerate() {
                for dynamic in [false, true] {
                    // PropGet has ordinary-read semantics; probes use DynamicPropGet at both sites.
                    let (function, _) = site(&mut module, &format!("read_{index}_{dynamic}"),
                        scope, property, dynamic || mode != PropertyFetchMode::Read, mode);
                    module.functions.push(function);
                }
            }
            let mut shared = SharedCodegenState::default();
            prepare(&module, &mut shared);
            let plan = shared.literal_property_read_helpers.as_ref().unwrap();
            assert_eq!(plan.helpers.len(), 4, "{target}");
            assert_eq!(plan.labels.len(), 4, "{target}");
        }
    }

    /// Equal names with distinct data ids denote one dispatch, rather than two single sites.
    #[test]
    fn literal_pool_positions_do_not_split_shared_semantics() {
        let mut module = Module::new(Target::parse("linux-x86_64").unwrap());
        let (first, _) = site(&mut module, "one", None, "value", false, PropertyFetchMode::Read);
        let (mut second, inst) = site(&mut module, "two", None, "value", false, PropertyFetchMode::Read);
        let duplicate = DataId::from_raw(module.data.strings.len() as u32);
        module.data.strings.push("value".to_string());
        second.instructions[inst.as_raw() as usize].immediate = Some(Immediate::Data(duplicate));
        module.functions.extend([first, second]);
        let mut shared = SharedCodegenState::default();
        prepare(&module, &mut shared);
        assert_eq!(shared.literal_property_read_helpers.as_ref().unwrap().helpers.len(), 1);
    }

    /// Quiet probes, hook recursion, helper recursion and borrowed results retain their old paths.
    #[test]
    fn unsafe_or_single_literal_sites_remain_inline() {
        let mut module = Module::new(Target::parse("linux-x86_64").unwrap());
        for name in ["one", "two"] {
            for (suffix, mode) in [("quiet", PropertyFetchMode::QuietRead), ("borrowed", PropertyFetchMode::Read)] {
                let (mut function, inst) = site(&mut module, &format!("{name}_{suffix}"),
                    None, suffix, true, mode);
                if suffix == "borrowed" {
                    function.instructions[inst.as_raw() as usize].result_ownership = Ownership::Borrowed;
                }
                module.functions.push(function);
            }
            for function_name in [format!("Box::__propget_{name}"), format!("{PREFIX}{name}")] {
                let (function, _) = site(&mut module, &function_name, None, "hidden", false, PropertyFetchMode::Read);
                module.functions.push(function);
            }
        }
        let (single, _) = site(&mut module, "single", None, "single", false, PropertyFetchMode::Read);
        module.functions.push(single);
        let mut shared = SharedCodegenState::default();
        prepare(&module, &mut shared);
        assert!(shared.literal_property_read_helpers.as_ref().unwrap().helpers.is_empty());
    }

    /// Inlined method accesses retain the callee scope even inside a different caller class.
    #[test]
    fn inlined_literal_sites_keep_their_original_visibility_scope() {
        let mut module = Module::new(Target::parse("linux-x86_64").unwrap());
        for name in ["one", "two"] {
            let (mut function, inst) = site(&mut module, name, Some("Caller"), "secret", false, PropertyFetchMode::Read);
            function.inlined_scopes.insert(inst, "Callee".to_string());
            module.class_methods.push(function);
        }
        let mut shared = SharedCodegenState::default();
        prepare(&module, &mut shared);
        let plan = shared.literal_property_read_helpers.as_ref().unwrap();
        assert_eq!(plan.helpers.len(), 1);
        assert_eq!(plan.helpers[0].0.scope.as_deref(), Some("Callee"));
    }

    /// The outlined operation retains borrowed input, owned output, literal identity and probe mode.
    #[test]
    fn literal_helper_uses_borrowed_frame_and_owned_original_operation() {
        let mut module = Module::new(Target::parse("linux-x86_64").unwrap());
        let data = module.data.intern_string("secret");
        let key = ReadKey { scope: Some("Vault".to_string()), property: "secret".to_string(), mode: PropertyFetchMode::Isset };
        let function = helper_function(&key, data, &format!("{PREFIX}0"));
        crate::ir::validate_function(&function).unwrap();
        assert!(function.flags.is_synthetic);
        assert_eq!(function.params.len(), 1);
        assert_eq!(function.lexical_class.as_deref(), Some("Vault"));
        assert!(function.no_epilogue_cleanup_slots.contains(&crate::ir::LocalSlotId::from_raw(0)));
        let read = function.instructions.iter().find(|inst| inst.op == Op::DynamicPropGet).unwrap();
        assert_eq!(read.immediate, Some(Immediate::PropertyFetchMode(PropertyFetchMode::Isset)));
        assert_eq!(read.result_ownership, Ownership::Owned);
        assert_eq!(function.value(read.operands[0]).unwrap().ownership, Ownership::Borrowed);
        // A source-less span would republish line 0 over the caller's call site.
        assert!(read.span.is_some_and(|span| !span.is_from_source()), "{read:?}");
    }

    /// No helper label or property data leaks from one independently emitted module to another.
    #[test]
    fn literal_plans_are_isolated_between_module_emissions() {
        for property in ["first", "second"] {
            let mut module = Module::new(Target::parse("linux-x86_64").unwrap());
            for name in ["one", "two"] {
                let (function, _) = site(&mut module, name, None, property, false, PropertyFetchMode::Read);
                module.functions.push(function);
            }
            let mut shared = SharedCodegenState::default();
            prepare(&module, &mut shared);
            let plan = shared.literal_property_read_helpers.as_ref().unwrap();
            assert_eq!(plan.helpers.len(), 1);
            assert_eq!(plan.helpers[0].0.property, property);
            assert_eq!(module.data.strings[plan.helpers[0].1.as_raw() as usize], property);
        }
    }
}

/// Builds a synthetic borrowed frame around the unchanged literal DynamicPropGet implementation.
fn helper_function(key: &ReadKey, data: DataId, label: &str) -> Function {
    let mixed = IrType::Heap(IrHeapKind::Mixed);
    let mut function = Function::new(label.to_string(), mixed, PhpType::Mixed);
    function.flags.is_synthetic = true;
    function.lexical_class = key.scope.clone();
    function.params.push(FunctionParam { name: "object".to_string(), ir_type: mixed,
        php_type: PhpType::Mixed, by_ref: false, variadic: false });
    let slot = function.add_local(Some("object".to_string()), mixed, PhpType::Mixed, LocalKind::BorrowedTemp);
    function.no_epilogue_cleanup_slots.insert(slot);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let object = builder.emit_load_local(slot, mixed, PhpType::Mixed);
    builder.set_value_ownership(object, Ownership::Borrowed);
    let name = builder.emit_const_str(data);
    // The caller's own instruction already published its source line and backtrace site. A
    // compiler-generated span keeps this inner read from republishing line 0 over them.
    let result = builder.emit_with_effects(Op::DynamicPropGet, vec![object, name],
        Some(Immediate::PropertyFetchMode(key.mode)), mixed, PhpType::Mixed, Ownership::Owned,
        Op::DynamicPropGet.default_effects(), Some(crate::span::Span::dummy()))
        .expect("property read produces a value");
    builder.terminate(Terminator::Return { value: Some(result) });
    function
}

/// Emits one ordinary EIR body per repeated literal dispatch, including its exception cleanup.
pub(super) fn emit_helpers(module: &Module, emitter: &mut Emitter, data: &mut DataSection,
    shared: &mut SharedCodegenState) -> Result<()> {
    prepare(module, shared);
    let helpers = shared.literal_property_read_helpers.as_ref().unwrap().helpers.clone();
    for (key, property, label) in helpers {
        let function = helper_function(&key, property, &label);
        crate::ir::validate_function(&function).map_err(|error|
            super::CodegenIrError::invalid_module(format!("shared literal property read: {error:?}")))?;
        emitter.comment(&format!("shared literal property read: scope={} property={:?} mode={}",
            key.scope.as_deref().unwrap_or("global"), key.property, key.mode.as_eir()));
        block_emit::emit_synthetic_function_with_label(module, &function, &label,
            emitter, data, shared, false)?;
    }
    Ok(())
}
