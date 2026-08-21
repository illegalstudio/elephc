//! Purpose:
//! Builds inline descriptor wrappers for runtime builtin and extern calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Emits a descriptor invoker inline and branches around its global entry body.
pub(super) fn emit_runtime_callable_invoker_inline(
    ctx: &mut FunctionContext<'_>,
    sig: &FunctionSig,
    captures: &[(String, PhpType, bool)],
) -> String {
    if let Some(label) = ctx.shared.runtime_callable_invoker(sig, captures) {
        return label;
    }
    let label = ctx.next_label("callable_invoker");
    let done_label = ctx.next_label("callable_invoker_done");
    let invoker = super::super::runtime_callable_invoker::RuntimeCallableInvoker {
        label: &label,
        sig,
        captures,
    };
    abi::emit_jump(ctx.emitter, &done_label);
    super::super::runtime_callable_invoker::emit_runtime_callable_invoker(ctx.emitter, ctx.data, &invoker);
    ctx.emitter.label(&done_label);
    ctx.shared
        .cache_runtime_callable_invoker(sig, captures, &label);
    label
}

/// Emits a synthetic EIR builtin wrapper so callable descriptors can use the PHP ABI.
pub(in crate::codegen) fn emit_runtime_builtin_wrapper_inline(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    sig: &FunctionSig,
    strict_php: bool,
) -> Result<String> {
    emit_runtime_call_wrapper_inline(
        ctx,
        name,
        sig,
        RuntimeCallWrapperKind::Builtin { strict_php },
    )
}

/// Returns the registry/runtime-descriptor ABI used by builtin callable wrappers.
pub(in crate::codegen) fn runtime_builtin_wrapper_sig(name: &str, sig: &FunctionSig) -> FunctionSig {
    let mut sig = sig.clone();
    if let Some(def) = crate::builtins::registry::lookup(name) {
        if let crate::builtins::semantics::BuiltinRuntimeFunctions::One(runtime_fn) =
            def.spec.semantics.runtime_functions
        {
            runtime_fn.refine_runtime_callable_wrapper_sig(&mut sig);
        }
    }
    sig
}

/// Emits an EIR extern wrapper inline so descriptors can point at PHP-ABI code.
pub(in crate::codegen) fn emit_runtime_extern_wrapper_inline(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    sig: &FunctionSig,
) -> Result<String> {
    emit_runtime_call_wrapper_inline(ctx, name, sig, RuntimeCallWrapperKind::Extern)
}

/// Kind of call instruction used by a descriptor entry wrapper.
#[derive(Clone, Copy)]
enum RuntimeCallWrapperKind {
    Builtin { strict_php: bool },
    Extern,
}

/// Emits a synthetic EIR wrapper that forwards PHP-ABI descriptor entry calls.
fn emit_runtime_call_wrapper_inline(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    sig: &FunctionSig,
    kind: RuntimeCallWrapperKind,
) -> Result<String> {
    let cached = match kind {
        RuntimeCallWrapperKind::Builtin { strict_php } => {
            ctx.shared.runtime_builtin_wrapper(name, sig, strict_php)
        }
        RuntimeCallWrapperKind::Extern => ctx.shared.runtime_extern_wrapper(name, sig),
    };
    if let Some(label) = cached {
        return Ok(label);
    }
    let label_prefix = match kind {
        RuntimeCallWrapperKind::Builtin { .. } => "callable_builtin",
        RuntimeCallWrapperKind::Extern => "callable_extern",
    };
    let label = ctx.next_label(label_prefix);
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    let mut wrapper_module = ctx.module.clone();
    let wrapper = build_runtime_call_wrapper_function(&mut wrapper_module, &label, name, sig, kind)?;
    abi::emit_jump(ctx.emitter, &done_label);
    super::super::block_emit::emit_synthetic_function_with_label(
        &wrapper_module,
        &wrapper,
        &label,
        ctx.emitter,
        ctx.data,
        ctx.shared,
        false,
    )?;
    ctx.emitter.label(&done_label);
    match kind {
        RuntimeCallWrapperKind::Builtin { strict_php } => {
            ctx.shared
                .cache_runtime_builtin_wrapper(name, sig, strict_php, &label)
        }
        RuntimeCallWrapperKind::Extern => {
            ctx.shared.cache_runtime_extern_wrapper(name, sig, &label)
        }
    }
    Ok(label)
}

/// Builds the EIR body for a PHP-ABI wrapper around a builtin or extern call.
fn build_runtime_call_wrapper_function(
    module: &mut Module,
    label: &str,
    name: &str,
    sig: &FunctionSig,
    kind: RuntimeCallWrapperKind,
) -> Result<Function> {
    let return_php_type = wrapper_return_php_type(&sig.return_type);
    let mut function = Function::new(
        label.to_string(),
        wrapper_return_ir_type(&return_php_type),
        return_php_type.clone(),
    );
    function.signature = Some(sig.clone());
    let params = wrapper_function_params(sig);
    function.params = params.clone();
    for param in params {
        function.add_local(
            Some(param.name.clone()),
            param.ir_type,
            param.php_type.clone(),
            LocalKind::PhpLocal,
        );
    }

    let data = module.data.intern_function_name(name);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let operands = wrapper_param_operands(&mut builder, sig);
    let result = match kind {
        RuntimeCallWrapperKind::Builtin { strict_php } => {
            let def = crate::builtins::registry::lookup(name).ok_or_else(|| {
                CodegenIrError::invalid_module(format!(
                    "callable wrapper {} is not registry-backed",
                    name,
                ))
            })?;
            let mut lowering = WrapperBuiltinLoweringContext {
                builder: &mut builder,
                strict_php,
            };
            Some(crate::builtins::semantics::lower_registry_call(
                &mut lowering,
                def,
                &operands,
                &return_php_type,
                crate::span::Span::dummy(),
            )
            .map_err(|error| {
                CodegenIrError::invalid_module(format!(
                    "callable wrapper lowering for {} failed: {}",
                    name, error,
                ))
            })?
            .value)
        }
        RuntimeCallWrapperKind::Extern => builder.emit(
            Op::ExternCall,
            operands,
            Some(Immediate::Data(data)),
            wrapper_return_ir_type(&return_php_type),
            return_php_type.clone(),
            Ownership::for_php_type(&return_php_type),
        ),
    };
    builder.terminate(Terminator::Return { value: result });
    Ok(function)
}

/// EIR construction adapter used by synthetic builtin callable wrappers.
struct WrapperBuiltinLoweringContext<'a, 'f> {
    builder: &'a mut Builder<'f>,
    strict_php: bool,
}

impl crate::builtins::semantics::BuiltinLoweringContext
    for WrapperBuiltinLoweringContext<'_, '_>
{
    /// Returns PHP metadata attached to one synthetic-wrapper operand.
    fn value_php_type(&self, value: ValueId) -> PhpType {
        self.builder.value_php_type(value)
    }

    /// Emits one backend-neutral operation into the synthetic wrapper body.
    fn emit_value(
        &mut self,
        op: Op,
        operands: Vec<ValueId>,
        immediate: Option<Immediate>,
        php_type: PhpType,
        effects: crate::ir::Effects,
        span: Option<crate::span::Span>,
    ) -> crate::builtins::semantics::LoweredBuiltinValue {
        let value = self
            .builder
            .emit_with_effects(
                op,
                operands,
                immediate,
                wrapper_value_ir_type(&php_type),
                php_type.clone(),
                Ownership::for_php_type(&php_type),
                effects,
                span,
            )
            .expect("builtin wrapper operation produces a value");
        crate::builtins::semantics::LoweredBuiltinValue { value }
    }

    /// Refuses to intern a PHP function name, which a synthetic wrapper cannot reach.
    ///
    /// Interning needs the module data pool, and this adapter only owns a function `Builder`.
    /// The only builtins that ask for it are the ones whose implementation is an injected
    /// prelude function — `sscanf()`/`fscanf()` — and those declare
    /// `BuiltinCallablePolicy::StaticOnly`, so no synthetic callable wrapper is ever built for
    /// them. A builtin reaching here would be one that lowers through `Op::Call` while claiming
    /// to be dynamically callable, which is a registry contradiction rather than a user error.
    fn intern_function_name(&mut self, name: &str) -> crate::ir::DataId {
        unreachable!(
            "a synthetic callable wrapper cannot intern the function name {name}: \
             builtins lowering through Op::Call must declare BuiltinCallablePolicy::StaticOnly"
        )
    }

    /// Emits one typed runtime operation into the synthetic wrapper body.
    fn emit_runtime_call(
        &mut self,
        target: crate::ir::RuntimeCallTarget,
        operands: Vec<ValueId>,
        php_type: PhpType,
        effects: crate::ir::Effects,
        span: Option<crate::span::Span>,
    ) -> crate::builtins::semantics::LoweredBuiltinValue {
        let target = match target {
            crate::ir::RuntimeCallTarget::Function(target) => {
                crate::ir::RuntimeCallTarget::ProfiledFunction {
                    target,
                    strict_php: self.strict_php,
                }
            }
            target => target,
        };
        self.emit_value(
            Op::RuntimeCall,
            operands,
            Some(Immediate::RuntimeCall(target)),
            php_type,
            effects,
            span,
        )
    }
}

/// Converts callable signature params into EIR function params with matching ABI/local slots.
pub(super) fn wrapper_function_params(sig: &FunctionSig) -> Vec<FunctionParam> {
    sig.params
        .iter()
        .enumerate()
        .map(|(idx, (name, php_type))| FunctionParam {
            name: name.clone(),
            ir_type: wrapper_value_ir_type(php_type),
            php_type: php_type.clone(),
            by_ref: sig.ref_params.get(idx).copied().unwrap_or(false),
            variadic: sig.variadic.as_deref() == Some(name.as_str()),
        })
        .collect()
}

/// Emits `LoadLocal` operands for every wrapper parameter.
pub(super) fn wrapper_param_operands(builder: &mut Builder<'_>, sig: &FunctionSig) -> Vec<ValueId> {
    sig.params
        .iter()
        .enumerate()
        .map(|(idx, (_, php_type))| {
            builder.emit_load_local(
                LocalSlotId::from_raw(idx as u32),
                wrapper_value_ir_type(php_type),
                php_type.clone(),
            )
        })
        .collect()
}

/// Returns a materializable PHP type for wrapper return values.
pub(super) fn wrapper_return_php_type(php_type: &PhpType) -> PhpType {
    match php_type.codegen_repr() {
        PhpType::Never => PhpType::Void,
        other => other,
    }
}

/// Returns EIR return storage for a wrapper function signature.
pub(super) fn wrapper_return_ir_type(php_type: &PhpType) -> IrType {
    match php_type.codegen_repr() {
        PhpType::Void | PhpType::Never => IrType::Void,
        other => IrType::from_php(&other),
    }
}

/// Returns EIR value storage for wrapper params and call results.
pub(super) fn wrapper_value_ir_type(php_type: &PhpType) -> IrType {
    match php_type.codegen_repr() {
        PhpType::Void | PhpType::Never => IrType::I64,
        other => IrType::from_php(&other),
    }
}
