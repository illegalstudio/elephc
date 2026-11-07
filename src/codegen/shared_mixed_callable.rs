//! Purpose:
//! Emits module-wide helpers for repeated open Mixed callable dispatch.
//! Replaces per-site string, callable-array, object, and descriptor ladders with one body.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()` before ordinary function bodies.
//! - `crate::codegen::lower_inst::callables` at eligible descriptor-invoke sites.
//!
//! Key details:
//! - Only unknown-name Mixed callbacks with raw `array<mixed>` arguments are shared.
//! - Eval-backed functions and finite callable-name sets retain their specialized inline paths.
//! - Strict and extension-enabled profiles use distinct helpers and descriptor catalogs.

use crate::codegen::callable_reachability::CallableReachabilityAnalysis;
use crate::codegen::context::FunctionContext;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::frame;
use crate::codegen::lower_inst;
use crate::codegen::shared_state::SharedCodegenState;
use crate::ir::{
    BasicBlock, BlockId, Function, FunctionParam, Immediate, InstId, Instruction, IrHeapKind,
    IrType, LocalKind, Module, Op, Ownership, Terminator, Value, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::Result;

/// Entry label for the extension-enabled open Mixed callable helper.
const MIXED_CALLABLE_LABEL: &str = "_eir_shared_mixed_callable_invoke";
/// Entry label for the strict-PHP open Mixed callable helper.
const STRICT_MIXED_CALLABLE_LABEL: &str = "_eir_shared_strict_mixed_callable_invoke";

/// The module-wide sharing decisions for open Mixed callable dispatch.
///
/// The dispatcher resolves `[$object, 'method']` and descriptor invocations against tables built
/// for the lexical class of the site, because php checks visibility in the scope the callable is
/// CALLED from: Symfony's `ErrorHandler::handleException()` calls the `[$this, 'renderException']`
/// it stored in a property, and that private method is reachable only from inside the class. One
/// helper for the whole module therefore serves only the sites whose scope cannot reach a
/// non-public method; a scope that can gets a helper of its own, emitted with that class as its
/// lexical scope.
#[derive(Clone, Default)]
pub(crate) struct MixedCallableSharing {
    /// The module-wide helper per strictness profile; `None` until decided.
    global: [Option<bool>; 2],
    /// Class names (no leading backslash) whose scope reaches a non-public method.
    sensitive: std::collections::HashSet<String>,
    /// `(strict, class id)` of every scope-specific helper that was emitted.
    scoped: std::collections::HashSet<(bool, u64)>,
}

/// Returns the lexical scope of one instruction, honoring spliced method bodies.
fn instruction_scope(function: &Function, index: usize) -> Option<&str> {
    match function.inlined_scopes.get(&InstId::from_raw(index as u32)) {
        Some(scope) => Some(scope.as_str()),
        None => function.lexical_class.as_deref(),
    }
}

/// `name` and its ancestors, without leading backslashes.
fn class_lineage<'a>(module: &'a Module, name: &'a str) -> impl Iterator<Item = &'a str> {
    std::iter::successors(Some(name.trim_start_matches('\\')), move |class| {
        module
            .class_infos
            .get(*class)
            .and_then(|info| info.parent.as_deref())
            .map(|parent| parent.trim_start_matches('\\'))
    })
}

/// Class names whose own scope can reach a `protected` or `private` method: the classes that
/// declare one, their ancestors (a parent reaches a protected method its descendant declares) and
/// their descendants (a class that made an inherited method public still reaches its siblings'
/// protected copy, through the prototype they share).
fn scope_sensitive_classes(module: &Module) -> std::collections::HashSet<String> {
    let mut declaring = std::collections::HashSet::new();
    for (name, info) in &module.class_infos {
        let non_public = info
            .method_visibilities
            .values()
            .chain(info.static_method_visibilities.values())
            .any(|visibility| !matches!(visibility, crate::parser::ast::Visibility::Public));
        if non_public {
            declaring.insert(name.trim_start_matches('\\'));
        }
    }
    let mut sensitive = std::collections::HashSet::new();
    for name in module.class_infos.keys() {
        if class_lineage(module, name).any(|class| declaring.contains(class)) {
            sensitive.insert(name.trim_start_matches('\\').to_string());
        }
    }
    for name in &declaring {
        sensitive.extend(class_lineage(module, name).map(str::to_string));
    }
    sensitive
}

/// The scope-specific helper class of a site: `Some` when its scope reaches non-public methods.
fn sensitive_scope<'a>(sharing: &MixedCallableSharing, scope: Option<&'a str>) -> Option<&'a str> {
    let scope = scope?.trim_start_matches('\\');
    sharing.sensitive.contains(scope).then_some(scope)
}

/// Returns the shared helper label for an eligible call site.
pub(super) fn shared_helper_label(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callable: ValueId,
    arg_container: ValueId,
) -> Option<String> {
    let strict_php = lower_inst::instruction_strict_php_profile(inst);
    let scope = ctx.lexical_class().map(str::to_string);
    let sharing = ctx.shared.mixed_callable_sharing();
    let label = match sensitive_scope(sharing, scope.as_deref()) {
        Some(class) => {
            let class_id = ctx.module.class_infos.get(class)?.class_id;
            if !sharing.scoped.contains(&(strict_php, class_id)) {
                return None;
            }
            scoped_helper_label(strict_php, class_id)
        }
        None => {
            if !sharing.global[profile_index(strict_php)].unwrap_or(false) {
                return None;
            }
            helper_label(strict_php).to_string()
        }
    };
    if ctx.function.name == label
        || function_has_eval_context(ctx.function)
        || !is_open_mixed_callable_site(ctx, callable, arg_container)
    {
        return None;
    }
    Some(label)
}

/// Emits each strictness-profile helper required by at least two eligible sites, and one per
/// class scope that reaches non-public methods and holds at least two.
pub(super) fn emit_shared_mixed_callable_helpers(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    let mut sharing = MixedCallableSharing {
        sensitive: scope_sensitive_classes(module),
        ..MixedCallableSharing::default()
    };
    let counts = mixed_callable_site_counts(module, &sharing.sensitive);
    for strict_php in [false, true] {
        let global = counts.get(&(strict_php, None)).copied().unwrap_or(0);
        sharing.global[profile_index(strict_php)] = Some(global >= 2);
    }
    let mut scoped_classes = counts
        .iter()
        .filter(|(_, count)| **count >= 2)
        .filter_map(|((strict_php, class), _)| {
            let class = class.as_deref()?;
            Some((*strict_php, class.to_string(), module.class_infos.get(class)?.class_id))
        })
        .collect::<Vec<_>>();
    scoped_classes.sort_by_key(|(strict_php, _, class_id)| (*class_id, *strict_php));
    for (strict_php, _, class_id) in &scoped_classes {
        sharing.scoped.insert((*strict_php, *class_id));
    }
    shared.set_mixed_callable_sharing(sharing.clone());
    for strict_php in [false, true] {
        if sharing.global[profile_index(strict_php)] == Some(true) {
            emit_one_helper(module, emitter, data, shared, regalloc_linear, strict_php, None)?;
        }
    }
    for (strict_php, class, _) in &scoped_classes {
        emit_one_helper(module, emitter, data, shared, regalloc_linear, *strict_php, Some(class))?;
    }
    Ok(())
}

/// Counts open Mixed callable sites per `(strictness, scope-specific class)` bucket; a `None`
/// class is the module-wide helper's bucket.
fn mixed_callable_site_counts(
    module: &Module,
    sensitive: &std::collections::HashSet<String>,
) -> std::collections::HashMap<(bool, Option<String>), usize> {
    let mut counts = std::collections::HashMap::new();
    for function in module_functions(module).filter(|function| !function_has_eval_context(function))
    {
        let reachability = CallableReachabilityAnalysis::new(module, function);
        for (index, inst) in function.instructions.iter().enumerate() {
            if inst.op != Op::CallableDescriptorInvoke {
                continue;
            }
            let Some((&callable, &arg_container)) = inst.operands.first().zip(inst.operands.get(1))
            else {
                continue;
            };
            if !site_types_are_shareable(function, callable, arg_container)
                || reachability.candidates(callable).is_some()
            {
                continue;
            }
            let strict_php = lower_inst::instruction_strict_php_profile(inst);
            let class = instruction_scope(function, index)
                .map(|scope| scope.trim_start_matches('\\'))
                .filter(|scope| sensitive.contains(*scope))
                .map(str::to_string);
            *counts.entry((strict_php, class)).or_insert(0) += 1;
        }
    }
    counts
}

/// Iterates every ordinary, method, and closure body emitted by the backend.
fn module_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
}

/// Returns whether a live context site has the shared helper's exact input shape.
fn is_open_mixed_callable_site(
    ctx: &FunctionContext<'_>,
    callable: ValueId,
    arg_container: ValueId,
) -> bool {
    site_types_are_shareable(ctx.function, callable, arg_container)
        && ctx.runtime_callable_candidates(callable).is_none()
}

/// Returns whether a site passes a boxed Mixed callback and raw Mixed-element array.
fn site_types_are_shareable(
    function: &Function,
    callable: ValueId,
    arg_container: ValueId,
) -> bool {
    let callback_ty = function
        .value(callable)
        .map(|value| value.php_type.codegen_repr());
    let container_ty = function
        .value(arg_container)
        .map(|value| value.php_type.codegen_repr());
    matches!(callback_ty, Some(PhpType::Mixed | PhpType::Union(_)))
        && matches!(
            container_ty,
            Some(PhpType::Array(element)) if element.codegen_repr() == PhpType::Mixed
        )
}

/// Returns whether a function owns a persistent eval context and needs its fallback path.
fn function_has_eval_context(function: &Function) -> bool {
    function
        .locals
        .iter()
        .any(|local| local.kind == LocalKind::EvalContext)
}

/// Emits one ordinary framed helper around the existing inline lowering routine.
fn emit_one_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
    strict_php: bool,
    scope: Option<&str>,
) -> Result<()> {
    let label = match scope.and_then(|class| module.class_infos.get(class)) {
        Some(info) => scoped_helper_label(strict_php, info.class_id),
        None => helper_label(strict_php).to_string(),
    };
    let label = label.as_str();
    let mut function = helper_function(label, strict_php);
    function.lexical_class = scope.map(str::to_string);
    let inst = function.instructions[0].clone();
    let layout = frame::layout_for_function(&function, emitter.target, regalloc_linear, false, false);
    let mut ctx = FunctionContext::new(
        module,
        &function,
        emitter,
        data,
        shared,
        layout,
        false,
        false,
        false,
        Some(format!("{}_epilogue", label)),
    );
    ctx.emitter.blank();
    ctx.emitter
        .comment(&format!("--- shared open Mixed callable dispatch: {} ---", label));
    frame::emit_function_prologue_with_label(&mut ctx, label)?;
    load_helper_param_value(&mut ctx, 0, callback_value())?;
    load_helper_param_value(&mut ctx, 1, arg_container_value())?;
    load_helper_param_value(&mut ctx, 2, kind_value())?;
    lower_inst::lower_mixed_callable_descriptor_invoke_inline(
        &mut ctx,
        &inst,
        callback_value(),
        arg_container_value(),
        "callable_descriptor_invoke",
    )?;
    ctx.load_value_to_result(result_value())?;
    // The invoker hands back a fresh Mixed cell, which this helper returns as it is.
    frame::emit_function_return_epilogue(
        &mut ctx,
        None,
        super::return_ownership::ReturnOwnershipStatus::Static(true),
    );
    Ok(())
}

/// Loads one borrowed helper parameter local into its synthetic SSA value home.
fn load_helper_param_value(
    ctx: &mut FunctionContext<'_>,
    raw_slot: u32,
    value: ValueId,
) -> Result<()> {
    ctx.load_raw_local_to_result(crate::ir::LocalSlotId::from_raw(raw_slot))?;
    ctx.store_result_value(value)
}

/// Builds the minimal EIR metadata needed by normal frame and callable lowering.
fn helper_function(label: &str, strict_php: bool) -> Function {
    let mixed_ir = IrType::Heap(IrHeapKind::Mixed);
    let array_ir = IrType::Heap(IrHeapKind::Array);
    let array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let entry = BlockId::from_raw(0);
    let inst_id = InstId::from_raw(0);
    let mut function = Function::new(label.to_string(), mixed_ir.clone(), PhpType::Mixed);
    function.flags.is_synthetic = true;
    function.params = vec![
        FunctionParam {
            name: "callback".to_string(),
            ir_type: mixed_ir.clone(),
            php_type: PhpType::Mixed,
            by_ref: false,
            variadic: false,
        },
        FunctionParam {
            name: "arguments".to_string(),
            ir_type: array_ir.clone(),
            php_type: array_ty.clone(),
            by_ref: false,
            variadic: false,
        },
        FunctionParam {
            name: "kind".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: false,
            variadic: false,
        },
    ];
    let callback_slot = function.add_local(
        Some("callback".to_string()),
        mixed_ir.clone(),
        PhpType::Mixed,
        LocalKind::BorrowedTemp,
    );
    let arguments_slot = function.add_local(
        Some("arguments".to_string()),
        array_ir.clone(),
        array_ty.clone(),
        LocalKind::BorrowedTemp,
    );
    let kind_slot = function.add_local(
        Some("kind".to_string()),
        IrType::I64,
        PhpType::Int,
        LocalKind::BorrowedTemp,
    );
    function.no_epilogue_cleanup_slots.insert(callback_slot);
    function.no_epilogue_cleanup_slots.insert(arguments_slot);
    function.no_epilogue_cleanup_slots.insert(kind_slot);
    function.values = vec![
        Value {
            ir_type: mixed_ir.clone(),
            php_type: PhpType::Mixed,
            def: ValueDef::BlockParam {
                block: entry,
                index: 0,
            },
            ownership: Ownership::Borrowed,
        },
        Value {
            ir_type: array_ir,
            php_type: array_ty,
            def: ValueDef::BlockParam {
                block: entry,
                index: 1,
            },
            ownership: Ownership::Borrowed,
        },
        Value {
            ir_type: mixed_ir.clone(),
            php_type: PhpType::Mixed,
            def: ValueDef::Instruction {
                block: entry,
                index: 0,
                inst: inst_id,
            },
            ownership: Ownership::Owned,
        },
        Value {
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            def: ValueDef::BlockParam {
                block: entry,
                index: 2,
            },
            ownership: Ownership::Borrowed,
        },
    ];
    function.instructions.push(Instruction::new(
        Op::CallableDescriptorInvoke,
        vec![callback_value(), arg_container_value()],
        Some(Immediate::Bool(strict_php)),
        Some(result_value()),
        mixed_ir,
        PhpType::Mixed,
        Ownership::Owned,
        Op::CallableDescriptorInvoke.default_effects(),
        None,
    ));
    let mut block = BasicBlock::new(
        entry,
        "entry".to_string(),
        vec![callback_value(), arg_container_value(), kind_value()],
    );
    block.instructions.push(inst_id);
    block.terminator = Some(Terminator::Return {
        value: Some(result_value()),
    });
    function.blocks.push(block);
    function.entry = entry;
    function
}

/// Returns the helper callback SSA value.
fn callback_value() -> ValueId {
    ValueId::from_raw(0)
}

/// Returns the helper argument-container SSA value.
fn arg_container_value() -> ValueId {
    ValueId::from_raw(1)
}

/// Returns the helper's callback-kind SSA value; see [`lower_inst::callback_kind_for_instruction`].
pub(super) fn kind_value() -> ValueId {
    ValueId::from_raw(3)
}

/// Returns whether `name` is one of the module-wide open Mixed callable helpers.
pub(super) fn is_shared_helper_name(name: &str) -> bool {
    name.starts_with(MIXED_CALLABLE_LABEL) || name.starts_with(STRICT_MIXED_CALLABLE_LABEL)
}

/// The label of the helper dedicated to one class scope.
fn scoped_helper_label(strict_php: bool, class_id: u64) -> String {
    format!("{}_c{}", helper_label(strict_php), class_id)
}

/// Returns the helper's owned Mixed result SSA value.
fn result_value() -> ValueId {
    ValueId::from_raw(2)
}

/// Returns the helper label for one strictness profile.
fn helper_label(strict_php: bool) -> &'static str {
    if strict_php {
        STRICT_MIXED_CALLABLE_LABEL
    } else {
        MIXED_CALLABLE_LABEL
    }
}

/// Maps one strictness profile to its shared-state slot.
fn profile_index(strict_php: bool) -> usize {
    usize::from(strict_php)
}
