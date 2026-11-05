//! Purpose:
//! Builds inline descriptor wrappers for runtime builtin and extern calls.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.
//! - Descriptor invokers bind raw user arguments; the hidden argc/count prefixes are
//!   synthesized inside the invoker body, never passed by the caller.
//! - Hidden invocation profiles preserve weak legacy scope ids and encode strict scope as -id-3.

use super::*;

/// Emits a descriptor invoker inline and branches around its global entry body.
pub(super) fn emit_runtime_callable_invoker_inline(
    ctx: &mut FunctionContext<'_>,
    sig: &FunctionSig,
    captures: &[(String, PhpType, bool)],
) -> String {
    emit_runtime_callable_invoker_with_string_owner(ctx, sig, captures, false, false)
}

/// Returns whether a wrapper over `strval` converts a gradual operand into its own detached copy.
///
/// `Cast` to string of a `Mixed`, union or object operand always allocates a fresh heap string
/// (the same `value_is_owning_mixed_string_cast` fact the AST lowering releases after use). A
/// string operand is the identity cast and stays borrowed, so only the gradual signature owns.
pub(in crate::codegen::lower_inst) fn builtin_wrapper_string_cast_is_owning(
    name: &str,
    sig: &FunctionSig,
) -> bool {
    name.eq_ignore_ascii_case("strval")
        && sig.return_type.codegen_repr() == PhpType::Str
        && sig.params.first().is_some_and(|(_, ty)| {
            matches!(
                ty.codegen_repr(),
                PhpType::Mixed | PhpType::Union(_) | PhpType::Object(_)
            )
        })
}

/// Emits a builtin invoker with mbstring's runtime argument boundary when required.
pub(super) fn emit_runtime_builtin_invoker_inline(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    sig: &FunctionSig,
) -> String {
    let operation = crate::builtins::registry::lookup(name).and_then(|definition| {
        if let crate::builtins::semantics::BuiltinRuntimeFunctions::One(target) =
            definition.spec.semantics.runtime_functions
        {
            target.mbstring_operation()
        } else {
            None
        }
    });
    let Some(operation) = operation else {
        // A transform that persists its result (`strtoupper`) must not be copied again here; the
        // `strval` wrapper over a gradual operand owns its detached cast as well. Either way the
        // invoker takes the string over instead of leaving the original behind, one block per
        // call through `$callable = 'strtoupper'; $callable($s)`.
        let owns_string_return = sig.return_type.codegen_repr() == PhpType::Str
            && (super::builtin_returns_owned_string(name)
                || builtin_wrapper_string_cast_is_owning(name, sig));
        return emit_runtime_callable_invoker_with_string_owner(
            ctx,
            sig,
            &[],
            owns_string_return,
            false,
        );
    };
    if operation == elephc_builtin_contract::RuntimeBuiltinId::MbConvertVariables {
        let label = ctx.next_global_label("callable_invoker");
        let defaults = crate::codegen::runtime_callable_invoker::resolve_invoker_defaults(
            ctx.module, None, sig,
        );
        let invoker = super::super::runtime_callable_invoker::RuntimeCallableInvoker {
            label: &label,
            sig,
            captures: &[],
            mbstring_operation: None,
            mbstring_variable_ref_warnings: true,
            owns_string_return: false,
            php_return_status: false,
            defaults: &defaults,
        };
        // Out of line, not spliced into the function that first needed it: see
        // `Emitter::begin_out_of_line` for the branch-range failure that splicing caused.
        let scope = ctx.emitter.begin_out_of_line();
        super::super::runtime_callable_invoker::emit_runtime_callable_invoker(
            ctx.emitter, ctx.data, &invoker,
        );
        ctx.emitter.end_out_of_line(scope);
        return label;
    }
    let label = ctx.next_global_label("callable_invoker");
    let defaults = crate::codegen::runtime_callable_invoker::resolve_invoker_defaults(
        ctx.module, None, sig,
    );
    let invoker = super::super::runtime_callable_invoker::RuntimeCallableInvoker {
        label: &label,
        sig,
        captures: &[],
        mbstring_operation: Some(operation),
        mbstring_variable_ref_warnings: false,
        owns_string_return: sig.return_type.codegen_repr() == PhpType::Str,
        php_return_status: false,
        defaults: &defaults,
    };
    // Out of line, not spliced into the function that first needed it: see
    // `Emitter::begin_out_of_line` for the branch-range failure that splicing caused.
    let scope = ctx.emitter.begin_out_of_line();
    super::super::runtime_callable_invoker::emit_runtime_callable_invoker(
        ctx.emitter, ctx.data, &invoker,
    );
    ctx.emitter.end_out_of_line(scope);
    label
}

/// Emits an invoker whose result-copy policy follows the concrete callee's string ownership.
pub(super) fn emit_runtime_callable_invoker_with_string_owner(
    ctx: &mut FunctionContext<'_>,
    sig: &FunctionSig,
    captures: &[(String, PhpType, bool)],
    owns_string_return: bool,
    php_return_status: bool,
) -> String {
    emit_runtime_callable_invoker_in_class(ctx, sig, captures, owns_string_return, php_return_status, None)
}

/// Emits a descriptor invoker whose defaults resolve in the DECLARING class's constant scope.
///
/// `self::`, `static::` and `parent::` in a parameter default answer to the class that declares
/// the callee, so a method invoker must name it. A free function passes `None`.
pub(super) fn emit_runtime_callable_invoker_in_class(
    ctx: &mut FunctionContext<'_>,
    sig: &FunctionSig,
    captures: &[(String, PhpType, bool)],
    owns_string_return: bool,
    php_return_status: bool,
    current_class: Option<&str>,
) -> String {
    emit_runtime_callable_invoker_inline_impl(
        ctx,
        sig,
        captures,
        owns_string_return,
        php_return_status,
        current_class,
        false,
    )
}

/// Emits a bounded invoker whose string-result policy follows the concrete callee, with
/// defaults resolved in the DECLARING class's constant scope (`None` for a free function).
pub(super) fn emit_runtime_callable_invoker_bounded_in_class(
    ctx: &mut FunctionContext<'_>,
    sig: &FunctionSig,
    captures: &[(String, PhpType, bool)],
    owns_string_return: bool,
    php_return_status: bool,
    current_class: Option<&str>,
) -> String {
    emit_runtime_callable_invoker_inline_impl(
        ctx,
        sig,
        captures,
        owns_string_return,
        php_return_status,
        current_class,
        true,
    )
}

/// Shared body of the inline invoker emitters.
///
/// An invoker shared through the cache takes its name from the cache key, so every worker that
/// reaches it spells the same symbol. The bounded form is never shared: it keeps a host-scoped
/// name, because nothing else may reference it, and it is not marked as a keyed helper, or the
/// merge would treat two different invokers as copies of one and drop a referenced definition.
fn emit_runtime_callable_invoker_inline_impl(
    ctx: &mut FunctionContext<'_>,
    sig: &FunctionSig,
    captures: &[(String, PhpType, bool)],
    owns_string_return: bool,
    php_return_status: bool,
    current_class: Option<&str>,
    catch_native_throws: bool,
) -> String {
    if crate::codegen::runtime_callable_invoker::needs_callable_argument_normalizer(sig) {
        ctx.shared.require_callable_argument_normalizer();
    }
    let defaults = crate::codegen::runtime_callable_invoker::resolve_invoker_defaults(
        ctx.module,
        current_class,
        sig,
    );
    if !catch_native_throws {
        if let Some(label) = ctx.shared.runtime_callable_invoker(
            sig,
            captures,
            owns_string_return,
            php_return_status,
            &defaults,
        ) {
            return label;
        }
    }
    let key = format!(
        "callable_invoker:{:?}:{:?}:{}:{}:{:?}",
        sig, captures, owns_string_return, php_return_status, defaults
    );
    let label = if catch_native_throws {
        ctx.next_global_label("callable_invoker")
    } else {
        ctx.keyed_global_label(&key, "callable_invoker")
    };
    let invoker = super::super::runtime_callable_invoker::RuntimeCallableInvoker {
        label: &label,
        sig,
        captures,
        mbstring_operation: None,
        mbstring_variable_ref_warnings: false,
        owns_string_return,
        php_return_status,
        defaults: &defaults,
    };
    // Out of line, not spliced into the function that first needed it: see
    // `Emitter::begin_out_of_line` for the branch-range failure that splicing caused. The shared
    // form is also a keyed helper, so the parallel merge keeps one copy across workers.
    let scope = ctx.emitter.begin_out_of_line();
    if catch_native_throws {
        super::super::runtime_callable_invoker::emit_runtime_callable_invoker_with_exception_boundary(
            ctx.emitter,
            ctx.data,
            &invoker,
        );
    } else {
        ctx.emit_keyed_helper(&key, |ctx| {
            super::super::runtime_callable_invoker::emit_runtime_callable_invoker(
                ctx.emitter,
                ctx.data,
                &invoker,
            );
        });
    }
    ctx.emitter.end_out_of_line(scope);
    if !catch_native_throws {
        ctx.shared.cache_runtime_callable_invoker(
            sig,
            captures,
            owns_string_return,
            php_return_status,
            &defaults,
            &label,
        );
    }
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
    let strictness = match kind {
        RuntimeCallWrapperKind::Builtin { strict_php } => strict_php,
        RuntimeCallWrapperKind::Extern => false,
    };
    // Keyed, so every codegen worker that reaches this wrapper spells the same symbol.
    let key = format!("{}:{}:{:?}:{}", label_prefix, name, sig, strictness);
    let label = ctx.keyed_global_label(&key, label_prefix);
    // Reserve the label before lowering the synthetic body. A callable-taking builtin
    // can itself require the open runtime callable table while its wrapper is being
    // emitted. Publishing the reservation here makes that recursion reuse this entry
    // instead of recursively cloning and lowering the same wrapper forever.
    match kind {
        RuntimeCallWrapperKind::Builtin { strict_php } => {
            ctx.shared
                .cache_runtime_builtin_wrapper(name, sig, strict_php, &label)
        }
        RuntimeCallWrapperKind::Extern => {
            ctx.shared.cache_runtime_extern_wrapper(name, sig, &label)
        }
    }
    // The wrapper is lowered against the module itself whenever every data entry its body names
    // is already interned -- the common case: an extern's own name, a builtin's messages. Only a
    // body that must intern something new, or one of the two builtins whose body is a full
    // semantic lowering, gets a private copy. Copying the WHOLE module (every body, every class)
    // for each of the hundreds of wrappers a callable-string table reaches was over half of the
    // codegen phase, serially, on Laravel and Symfony.
    let module = ctx.module;
    let mut copied_module = None;
    let read_only = if wrapper_needs_full_lowering(name, sig, kind) {
        None
    } else {
        let mut missed = false;
        let wrapper = build_runtime_call_wrapper_function(
            WrapperData::ReadOnly { module, missed: &mut missed },
            &label,
            name,
            sig,
            kind,
        )?;
        (!missed).then_some(wrapper)
    };
    let wrapper = match read_only {
        Some(wrapper) => wrapper,
        None => {
            let mut owned = ctx.shared.take_wrapper_module(module);
            let wrapper = build_runtime_call_wrapper_function(
                WrapperData::Owned(&mut owned),
                &label,
                name,
                sig,
                kind,
            );
            copied_module = Some(owned);
            wrapper?
        }
    };
    let wrapper_module: &crate::ir::Module = copied_module.as_deref().unwrap_or(module);
    ctx.shared.record_generated_runtime_features(&wrapper);
    // Out of line, not spliced into the function that first needed it: see
    // `Emitter::begin_out_of_line` for the branch-range failure that splicing caused. Keyed, so
    // the parallel merge keeps one copy of the wrapper across workers.
    let scope = ctx.emitter.begin_out_of_line();
    let emitted = ctx.emit_keyed_helper(&key, |ctx| {
        super::super::block_emit::emit_synthetic_function_with_label(
            wrapper_module,
            &wrapper,
            &label,
            ctx.emitter,
            ctx.data,
            ctx.shared,
            false,
        )
    });
    // Closed on the error path too: the scope holds the caller's text, and returning early with
    // it would drop everything the caller had emitted so far.
    ctx.emitter.end_out_of_line(scope);
    emitted?;
    if let Some(copy) = copied_module {
        ctx.shared.return_wrapper_module(copy);
    }
    Ok(label)
}

/// Where a wrapper body's data entries come from.
///
/// `ReadOnly` looks every entry up in the module as it stands and flags a miss instead of
/// interning; `Owned` interns into a private copy of the module.
enum WrapperData<'a> {
    ReadOnly { module: &'a crate::ir::Module, missed: &'a mut bool },
    Owned(&'a mut crate::ir::Module),
}

impl WrapperData<'_> {
    /// Returns the id of a string literal, interning it only into an owned copy.
    fn string(&mut self, value: &str) -> crate::ir::DataId {
        match self {
            Self::Owned(module) => module.data.intern_string(value),
            Self::ReadOnly { module, missed } => module.data.string_id(value).unwrap_or_else(|| {
                **missed = true;
                crate::ir::DataId::from_raw(0)
            }),
        }
    }

    /// Returns the id of a function name, interning it only into an owned copy.
    fn function_name(&mut self, value: &str) -> crate::ir::DataId {
        match self {
            Self::Owned(module) => module.data.intern_function_name(value),
            Self::ReadOnly { module, missed } => {
                module.data.function_name_id(value).unwrap_or_else(|| {
                    **missed = true;
                    crate::ir::DataId::from_raw(0)
                })
            }
        }
    }
}

/// Returns whether a builtin wrapper's body is a full semantic lowering, which interns freely.
fn wrapper_needs_full_lowering(name: &str, sig: &FunctionSig, kind: RuntimeCallWrapperKind) -> bool {
    let RuntimeCallWrapperKind::Builtin { .. } = kind else {
        return false;
    };
    let runtime_function = |id| {
        crate::builtins::registry::lookup(name).is_some_and(|def| {
            def.spec.semantics.runtime_functions
                == crate::builtins::semantics::BuiltinRuntimeFunctions::One(id)
        })
    };
    crate::builtins::registry::lookup(name).is_some_and(|def| matches!(
        def.spec.semantics.argument_lowering,
        crate::builtins::semantics::BuiltinArgumentLowering::WeakStringUnion { .. }
    )) || (runtime_function(crate::ir::RuntimeFnId::ArrayMerge) && sig.variadic.is_some())
        || (runtime_function(crate::ir::RuntimeFnId::Usort)
            && sig.params.first().is_some_and(|(_, ty)| ty.codegen_repr() == PhpType::Mixed))
}

/// Builds the EIR body for a PHP-ABI wrapper around a builtin or extern call.
fn build_runtime_call_wrapper_function(
    mut data: WrapperData<'_>,
    label: &str,
    name: &str,
    sig: &FunctionSig,
    kind: RuntimeCallWrapperKind,
) -> Result<Function> {
    if let (RuntimeCallWrapperKind::Builtin { strict_php }, WrapperData::Owned(module)) =
        (kind, &mut data)
    {
        let module: &mut crate::ir::Module = module;
        // GMP's homes lower through the prelude helper graph, which first-class callable
        // wrappers must reproduce. Other EIR-graph homes already build their own callable
        // body through the generic wrapper path, so widening this predicate would duplicate
        // their allocation graph (for example `gc_status`).
        let prepared_eir_lowering = crate::builtins::registry::lookup(name).is_some_and(|def| {
            def.spec.semantics.runtime_functions
                == crate::builtins::semantics::BuiltinRuntimeFunctions::None
                && matches!(
                    def.spec.semantics.lowering,
                    crate::builtins::semantics::BuiltinLowering::Eir(_)
                )
                && elephc_builtin_contract::lookup(name)
                    .is_some_and(|contract| contract.module == elephc_builtin_contract::PhpModule::Gmp)
        });
        if prepared_eir_lowering || crate::builtins::registry::lookup(name).is_some_and(|def| matches!(
            def.spec.semantics.argument_lowering,
            crate::builtins::semantics::BuiltinArgumentLowering::WeakStringUnion { .. }
        )) {
            let function = crate::ir_lower::lower_prepared_builtin_callable(
                module, label, name, sig, strict_php,
            );
            crate::ir::validate_function(&function).map_err(|error|
                CodegenIrError::invalid_module(format!("prepared builtin callable: {error:?}")))?;
            return Ok(function);
        }
        let packed_merge = crate::builtins::registry::lookup(name).is_some_and(|def| {
            def.spec.semantics.runtime_functions
                == crate::builtins::semantics::BuiltinRuntimeFunctions::One(crate::ir::RuntimeFnId::ArrayMerge)
        }) && sig.variadic.is_some();
        if packed_merge {
            // Keep the public variadic signature, but validate and unpack it before
            // crossing the two-operand backend boundary. Full body lowering owns the pack.
            let function = crate::ir_lower::lower_array_merge_callable(module, label, sig, strict_php);
            crate::ir::validate_function(&function).map_err(|error| {
                CodegenIrError::invalid_module(format!("array merge callable wrapper: {error:?}"))
            })?;
            return Ok(function);
        }
        let boxed_user_sort = crate::builtins::registry::lookup(name).is_some_and(|def| {
            def.spec.semantics.runtime_functions
                == crate::builtins::semantics::BuiltinRuntimeFunctions::One(crate::ir::RuntimeFnId::Usort)
        }) && sig.params.first().is_some_and(|(_, ty)| ty.codegen_repr() == PhpType::Mixed);
        if boxed_user_sort {
            // The graph owns reference capture and normal/exceptional publication. A raw
            // RuntimeCall would bypass that graph and treat a Mixed cell as an array header.
            let function = crate::ir_lower::lower_boxed_usort_callable(module, label, sig, strict_php);
            crate::ir::validate_function(&function).map_err(|error| {
                CodegenIrError::invalid_module(format!("boxed sort callable wrapper: {error:?}"))
            })?;
            return Ok(function);
        }
    }
    let carries_invocation_scope = matches!(
        kind,
        RuntimeCallWrapperKind::Builtin { .. }
    ) && crate::builtins::registry::lookup(name).is_some_and(|def| {
        def.spec.semantics.runtime_functions
            == crate::builtins::semantics::BuiltinRuntimeFunctions::One(
                crate::ir::RuntimeFnId::CloneWith,
            )
    });
    let return_php_type = wrapper_return_php_type(&sig.return_type);
    let mut function = Function::new(
        label.to_string(),
        wrapper_return_ir_type(&return_php_type),
        return_php_type.clone(),
    );
    function.signature = Some(sig.clone());
    // Lets typed runtime lowering know its string result feeds a descriptor invoker owner.
    function.flags.is_callback_wrapper = matches!(kind, RuntimeCallWrapperKind::Builtin { .. });
    let mut params = wrapper_function_params(sig);
    if carries_invocation_scope {
        params.push(FunctionParam {
            name: "__elephc_invocation_scope".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: false,
            variadic: false,
        });
    }
    function.params = params.clone();
    for param in params {
        function.add_local(
            Some(param.name.clone()),
            param.ir_type,
            param.php_type.clone(),
            LocalKind::PhpLocal,
        );
    }

    let extern_name = matches!(kind, RuntimeCallWrapperKind::Extern).then(|| data.function_name(name));
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let mut operands = wrapper_param_operands(&mut builder, sig);
    if carries_invocation_scope {
        let profile = builder.emit_load_local(
            LocalSlotId::from_raw(sig.params.len() as u32),
            IrType::I64,
            PhpType::Int,
        );
        operands.push(decode_invocation_scope(&mut builder, profile));
    }
    let pointer_op = match kind {
        RuntimeCallWrapperKind::Builtin { .. } => crate::builtins::registry::lookup(name).and_then(
            |def| match def.spec.semantics.argument_lowering {
                crate::builtins::semantics::BuiltinArgumentLowering::ArrayInternalPointer(op) => {
                    Some(op)
                }
                _ => None,
            },
        ),
        RuntimeCallWrapperKind::Extern => None,
    };
    let result = if let Some(op) = pointer_op {
        // A descriptor call hands the pointer builtins a fresh array value, so there is no
        // cursor slot to read: start at the first position, exactly as the direct-call lowering
        // does for a non-local receiver (`lower_array_internal_pointer`).
        let container = operands[0];
        let cursor = builder.emit_const_i64(0);
        let cursor = match op.seek_mode() {
            None => cursor,
            Some(mode) => {
                let mode = builder.emit_const_i64(mode);
                builder
                    .emit_with_effects(
                        Op::RuntimeCall,
                        vec![container, cursor, mode],
                        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
                            crate::ir::RuntimeFnId::ArrayPtrSeek,
                        ))),
                        IrType::I64,
                        PhpType::Int,
                        Ownership::NonHeap,
                        crate::ir::RuntimeFnId::ArrayPtrSeek.effects(),
                        None,
                    )
                    .expect("array_ptr_seek produces a value")
            }
        };
        let target = if op.reads_key() {
            crate::ir::RuntimeFnId::ArrayPtrKey
        } else {
            crate::ir::RuntimeFnId::ArrayPtrValue
        };
        let value = builder
            .emit_with_effects(
                Op::RuntimeCall,
                vec![container, cursor],
                Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))),
                wrapper_return_ir_type(&return_php_type),
                return_php_type.clone(),
                Ownership::Owned,
                target.effects(),
                None,
            )
            .expect("array pointer read produces a value");
        Some(value)
    } else {
    match kind {
        RuntimeCallWrapperKind::Builtin { strict_php } => {
            let def = crate::builtins::registry::lookup(name).ok_or_else(|| {
                CodegenIrError::invalid_module(format!(
                    "callable wrapper {} is not registry-backed",
                    name,
                ))
            })?;
            let operands = coerce_gradual_wrapper_operands(&mut builder, name, sig, operands);
            let mut lowering = WrapperBuiltinLoweringContext {
                builder: &mut builder,
                data: &mut data,
                strict_php,
            };
            let result = crate::builtins::semantics::lower_registry_call(
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
            .value;
            if builtin_wrapper_string_cast_is_owning(name, sig) {
                // The gradual cast owns a detached string (heap for a string payload, scratch for
                // a number). Persisting it hands the invoker one heap owner whatever the payload
                // was, and the cast's own copy is released here instead of being abandoned.
                builder.set_value_ownership(result, Ownership::Owned);
                let persisted = builder
                    .emit_with_effects(
                        Op::StrPersist,
                        vec![result],
                        None,
                        IrType::Str,
                        PhpType::Str,
                        Ownership::Owned,
                        Op::StrPersist.default_effects(),
                        None,
                    )
                    .expect("str_persist produces a value");
                builder.emit_with_effects(
                    Op::Release,
                    vec![result],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                    Op::Release.default_effects(),
                    None,
                );
                Some(persisted)
            } else {
                let ownership = wrapper_result_ownership(
                    def.spec.semantics.result_ownership,
                    &return_php_type,
                    name,
                )?;
                builder.set_value_ownership(result, ownership);
                Some(result)
            }
        }
        RuntimeCallWrapperKind::Extern => {
            let result = builder.emit(
                Op::ExternCall,
                operands,
                Some(Immediate::Data(
                    extern_name.expect("an extern wrapper interns its callee's name"),
                )),
                wrapper_return_ir_type(&return_php_type),
                return_php_type.clone(),
                if Ownership::php_type_needs_lifetime_tracking(&return_php_type) {
                    Ownership::Borrowed
                } else {
                    Ownership::NonHeap
                },
            );
            result
        }
    }
    };
    builder.terminate(Terminator::Return { value: result });
    Ok(function)
}

/// Removes source strictness before the clone wrapper consumes its lexical class id.
fn decode_invocation_scope(builder: &mut Builder<'_>, profile: ValueId) -> ValueId {
    let global = builder.emit_const_i64(-1);
    let strict = builder.emit(Op::ICmp, vec![profile, global],
        Some(Immediate::CmpPredicate(crate::ir::CmpPredicate::Slt)),
        IrType::I64, PhpType::Bool, Ownership::NonHeap).unwrap();
    let decode = builder.create_named_block("scope.strict", Vec::new());
    let merge = builder.create_named_block("scope.decoded", vec![(IrType::I64, PhpType::Int)]);
    builder.terminate(Terminator::CondBr {
        cond: strict, then_target: decode, then_args: Vec::new(),
        else_target: merge, else_args: vec![profile],
    });
    builder.position_at_end(decode);
    let base = builder.emit_const_i64(-3);
    let decoded = builder.emit(Op::ISub, vec![base, profile], None,
        IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap();
    builder.terminate(Terminator::Br { target: merge, args: vec![decoded] });
    builder.position_at_end(merge);
    builder.block_param(merge, 0)
}

/// Casts a GRADUAL wrapper operand to the parameter type the builtin's own lowering needs.
///
/// A callable wrapper's parameters ARE the source element types: the descriptor case is
/// specialized to the array it will walk. A `Mixed` element therefore reached, say, `strtolower`'s
/// lowering, which needs a concrete string -- so the only way to keep the wrapper lowerable was to
/// refuse every gradual source up front (`callable_accepts_string_source`), and
/// `array_map('strtolower', $gradual)` compiled into a runtime abort:
/// `callback string does not name a supported callable`. php casts there, and so does this.
///
/// Only a SCALAR target is cast. A container or object parameter has no single cast, and the
/// builtins that take one already accept a gradual source through their own policy.
fn coerce_gradual_wrapper_operands(
    builder: &mut Builder<'_>,
    name: &str,
    sig: &FunctionSig,
    operands: Vec<ValueId>,
) -> Vec<ValueId> {
    let Some(declared) = crate::types::first_class_callable_builtin_sig(name) else {
        return operands;
    };
    operands
        .into_iter()
        .enumerate()
        .map(|(idx, operand)| {
            let source = sig.params.get(idx).map(|(_, ty)| ty.codegen_repr());
            let target = declared.params.get(idx).map(|(_, ty)| ty.codegen_repr());
            let (Some(PhpType::Mixed), Some(target)) = (source, target) else {
                return operand;
            };
            if !matches!(
                target,
                PhpType::Str | PhpType::Int | PhpType::Float | PhpType::Bool
            ) {
                return operand;
            }
            builder
                .emit(
                    Op::RuntimeCall,
                    vec![operand],
                    None,
                    wrapper_value_ir_type(&target),
                    target.clone(),
                    Ownership::for_php_type(&target),
                )
                .unwrap_or(operand)
        })
        .collect()
}

/// Returns whether a builtin's wrapper can state its result ownership for `return_ty`.
///
/// A builtin whose result aliases its arguments only on SOME paths has no single EIR ownership,
/// so no wrapper exists for it. A dispatch table over every builtin (an open runtime callable
/// name) skips those rather than failing the whole build on one it will never be asked for.
pub(crate) fn builtin_wrapper_result_is_representable(name: &str, return_ty: &PhpType) -> bool {
    crate::builtins::registry::lookup(name).is_some_and(|def| {
        wrapper_result_ownership(def.spec.semantics.result_ownership, return_ty, name).is_ok()
    })
}

/// Maps a builtin's shared ownership contract onto its synthetic wrapper result.
fn wrapper_result_ownership(
    contract: crate::builtins::semantics::BuiltinResultOwnership,
    return_ty: &PhpType,
    name: &str,
) -> Result<Ownership> {
    use crate::builtins::semantics::BuiltinResultOwnership;
    if !Ownership::php_type_needs_lifetime_tracking(return_ty) {
        return Ok(Ownership::NonHeap);
    }
    match contract {
        BuiltinResultOwnership::Fresh => Ok(Ownership::Owned),
        BuiltinResultOwnership::Borrowed | BuiltinResultOwnership::Aliases(_) => {
            Ok(Ownership::Borrowed)
        }
        BuiltinResultOwnership::MayAliasArguments
            if return_ty.codegen_repr() == PhpType::Str =>
        {
            // String wrapper returns are persisted before the native C boundary,
            // so aliasing and fresh scratch paths share one borrowed EIR convention.
            Ok(Ownership::Borrowed)
        }
        BuiltinResultOwnership::MayAliasArguments => {
            Err(CodegenIrError::invalid_module(format!(
                "callable wrapper {} needs a runtime marker for path-dependent ownership of {:?}",
                name, return_ty
            )))
        }
        BuiltinResultOwnership::Independent if return_ty.codegen_repr() == PhpType::Str => {
            Ok(Ownership::Borrowed)
        }
        // A boxed `mixed` result (`preg_replace()` over operands that may be arrays, whose
        // callable wrapper answers `string|array|null`) is a cell the runtime call just built and
        // no argument backs: the wrapper's caller owns it, exactly as the direct call's result is.
        // Leaving it ambiguous dropped the builtin from every runtime-string dispatch table, so
        // `$replace = $cb ? 'preg_replace_callback' : 'preg_replace'; $replace(...)` (Symfony's
        // `ByteString::replaceMatches()`) had no descriptor target.
        BuiltinResultOwnership::Independent
            if matches!(return_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) =>
        {
            Ok(Ownership::Owned)
        }
        BuiltinResultOwnership::Independent | BuiltinResultOwnership::NonHeap => {
            Err(CodegenIrError::invalid_module(format!(
                "callable wrapper {} has ambiguous {:?} ownership for {:?}",
                name, contract, return_ty
            )))
        }
    }
}

/// EIR construction adapter used by synthetic builtin callable wrappers.
struct WrapperBuiltinLoweringContext<'a, 'f, 'd> {
    builder: &'a mut Builder<'f>,
    data: &'a mut WrapperData<'d>,
    strict_php: bool,
}

impl crate::builtins::semantics::BuiltinLoweringContext
    for WrapperBuiltinLoweringContext<'_, '_, '_>
{
    /// Returns PHP metadata attached to one synthetic-wrapper operand.
    fn value_php_type(&self, value: ValueId) -> PhpType {
        self.builder.value_php_type(value)
    }

    /// Rejects class-name interning because synthetic wrappers do not own the module data pool.
    fn intern_class_name(&mut self, _value: &str) -> crate::ir::DataId {
        panic!("static-only builtin lowering cannot intern class names in a callable wrapper")
    }

    /// Interns an ordinary string into the synthetic wrapper module's data pool.
    fn intern_string(&mut self, value: &str) -> crate::ir::DataId {
        self.data.string(value)
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
        // Synthetic wrappers bypass AST lowering's final ownership analysis.
        // Allocation primitives still transfer their fresh owner to their consumer.
        let ownership = if matches!(op, Op::MixedBox | Op::ArrayNew | Op::HashNew) {
            Ownership::Owned
        } else {
            Ownership::for_php_type(&php_type)
        };
        let value = self
            .builder
            .emit_with_effects(
                op,
                operands,
                immediate,
                wrapper_value_ir_type(&php_type),
                php_type.clone(),
                ownership,
                effects,
                span,
            )
            .expect("builtin wrapper operation produces a value");
        crate::builtins::semantics::LoweredBuiltinValue { value }
    }

    /// Emits one void operation into the synthetic wrapper body.
    fn emit_void(
        &mut self,
        op: Op,
        operands: Vec<ValueId>,
        immediate: Option<Immediate>,
        effects: crate::ir::Effects,
        span: Option<crate::span::Span>,
    ) {
        self.builder.emit_with_effects(
            op,
            operands,
            immediate,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            effects,
            span,
        );
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
                    arguments: crate::ir::RuntimeArgumentLayout::Values,
                    strict_php: self.strict_php,
                    strict_types: None,
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

    /// A synthetic wrapper has no function-name pool to intern a callee into, and no
    /// registry builtin that composes user calls admits a runtime-selected callable
    /// (`BuiltinCallablePolicy::StaticOnly`), so this is never reached; it answers a
    /// null constant rather than a call so a future policy change fails visibly in tests
    /// instead of emitting an unresolved symbol.
    fn emit_user_call(
        &mut self,
        _name: &str,
        _operands: Vec<ValueId>,
        php_type: PhpType,
        span: Option<crate::span::Span>,
    ) -> crate::builtins::semantics::LoweredBuiltinValue {
        self.emit_value(
            Op::ConstNull,
            Vec::new(),
            None,
            php_type,
            Op::ConstNull.default_effects(),
            span,
        )
    }

    /// A wrapper's operands are its own parameters, never a caller's locals, so there is
    /// no by-reference output to write; the composing builtin reports the failure.
    fn store_operand_local(
        &mut self,
        _operand: ValueId,
        _value: ValueId,
        _php_type: PhpType,
        _span: Option<crate::span::Span>,
    ) -> bool {
        false
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The public callable signature stays at two PHP arguments while the clone wrapper alone
    /// consumes the descriptor invoker's trailing invocation-scope id.
    #[test]
    fn clone_callable_wrapper_consumes_hidden_invocation_scope_on_all_targets() {
        for target in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut module = crate::ir::Module::new(crate::codegen::platform::Target::parse(target).unwrap());
            let sig = crate::builtins::registry::first_class_callable_sig("clone").unwrap();
            let wrapper = build_runtime_call_wrapper_function(
                WrapperData::Owned(&mut module),
                "clone_scope_probe",
                "clone",
                &sig,
                RuntimeCallWrapperKind::Builtin { strict_php: false },
            )
            .unwrap();
            assert_eq!(sig.params.len(), 2, "{target}");
            assert_eq!(wrapper.signature.as_ref().unwrap().params.len(), 2, "{target}");
            assert_eq!(wrapper.params.len(), 3, "{target}");
            assert_eq!(wrapper.params[2].name, "__elephc_invocation_scope", "{target}");
            let call = wrapper
                .instructions
                .iter()
                .find(|inst| {
                    matches!(
                        inst.immediate,
                        Some(Immediate::RuntimeCall(
                            crate::ir::RuntimeCallTarget::ProfiledFunction {
                                target: crate::ir::RuntimeFnId::CloneWith,
                                ..
                            }
                        ))
                    )
                })
                .expect("clone wrapper RuntimeCall");
            assert_eq!(call.operands.len(), 3, "{target}");
            assert_eq!(wrapper.value(call.operands[2]).unwrap().php_type, PhpType::Int);
        }
    }

    /// The callable signature and wrapper keep `debug_backtrace()` as a raw PHP array.
    #[test]
    fn debug_backtrace_callable_wrapper_preserves_array_storage() {
        let expected = PhpType::Array(Box::new(PhpType::Mixed));
        let sig = crate::builtins::registry::first_class_callable_sig("debug_backtrace").unwrap();
        assert_eq!(sig.return_type, expected);

        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut module = crate::ir::Module::new(crate::codegen::platform::Target::parse(target).unwrap());
            let wrapper = build_runtime_call_wrapper_function(
                WrapperData::Owned(&mut module),
                "debug_backtrace_array_probe",
                "debug_backtrace",
                &sig,
                RuntimeCallWrapperKind::Builtin { strict_php: false },
            ).unwrap();
            assert_eq!(wrapper.return_php_type, expected, "{target}");
            assert_eq!(
                wrapper.return_type,
                IrType::Heap(crate::ir::IrHeapKind::Array),
                "{target}",
            );

            let result = wrapper.instructions.iter()
                .find(|inst| inst.op == Op::CoreBuiltin)
                .and_then(|inst| inst.result)
                .expect("debug_backtrace wrapper must emit one Core builtin result");
            let value = wrapper.value(result).expect("Core builtin result value");
            assert_eq!(value.php_type, expected, "{target}");
            assert_eq!(
                value.ir_type,
                IrType::Heap(crate::ir::IrHeapKind::Array),
                "{target}",
            );
        }
    }

    /// GC wrappers transfer all fresh cells instead of retaining them again at each hash insert.
    #[test]
    fn gc_status_callable_wrapper_marks_all_allocations_owned() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut module = crate::ir::Module::new(crate::codegen::platform::Target::parse(target).unwrap());
            let sig = crate::builtins::registry::first_class_callable_sig("gc_status").unwrap();
            let wrapper = build_runtime_call_wrapper_function(
                WrapperData::Owned(&mut module), "gc_status_owner_probe", "gc_status", &sig,
                RuntimeCallWrapperKind::Builtin { strict_php: false },
            ).unwrap();
            let allocations: Vec<_> = wrapper.instructions.iter()
                .filter(|inst| matches!(inst.op, Op::HashNew | Op::MixedBox))
                .collect();
            assert_eq!(allocations.len(), 14, "{target}");
            for allocation in allocations {
                assert_eq!(allocation.result_ownership, Ownership::Owned, "{target}: {:?}", allocation.op);
                assert_eq!(wrapper.value(allocation.result.unwrap()).unwrap().ownership, Ownership::Owned);
            }
            assert!(wrapper.instructions.iter().any(|inst| inst.op == Op::Release), "{target}");
        }
    }

    /// A wrapper whose body only names data the module already interned is built against the
    /// module as it stands -- no copy -- and comes out identical to the copied build; one that
    /// needs a new entry reports the miss so the caller falls back to a private copy. Copying the
    /// whole module per wrapper was over half of the Laravel and Symfony codegen phase.
    #[test]
    fn read_only_wrapper_build_matches_the_owned_build_or_reports_a_miss() {
        let target = crate::codegen::platform::Target::parse("macos-aarch64").unwrap();
        let sig = FunctionSig {
            params: vec![("value".to_string(), PhpType::Int)],
            param_type_exprs: vec![None],
            param_attributes: vec![Vec::new()],
            defaults: vec![None],
            return_type: PhpType::Int,
            declared_return: true,
            by_ref_return: false,
            ref_params: vec![false],
            declared_params: vec![true],
            variadic: None,
            deprecation: None,
            is_generator: false,
        };
        let mut owned_module = crate::ir::Module::new(target);
        let owned = build_runtime_call_wrapper_function(
            WrapperData::Owned(&mut owned_module),
            "extern_probe",
            "probe_extern",
            &sig,
            RuntimeCallWrapperKind::Extern,
        )
        .unwrap();

        let mut missed = false;
        let fresh = crate::ir::Module::new(target);
        build_runtime_call_wrapper_function(
            WrapperData::ReadOnly { module: &fresh, missed: &mut missed },
            "extern_probe",
            "probe_extern",
            &sig,
            RuntimeCallWrapperKind::Extern,
        )
        .unwrap();
        assert!(missed, "an extern name the module never interned must be reported");

        let mut missed = false;
        let read_only = build_runtime_call_wrapper_function(
            WrapperData::ReadOnly { module: &owned_module, missed: &mut missed },
            "extern_probe",
            "probe_extern",
            &sig,
            RuntimeCallWrapperKind::Extern,
        )
        .unwrap();
        assert!(!missed);
        assert_eq!(format!("{:?}", read_only.instructions), format!("{:?}", owned.instructions));
    }
}
