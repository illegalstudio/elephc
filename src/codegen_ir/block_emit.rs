//! Purpose:
//! Walks EIR basic blocks in function order and delegates instruction/terminator lowering.
//! Owns function setup for the initial Phase 04 backend path.
//!
//! Called from:
//! - `crate::codegen_ir::generate_user_asm_from_ir()`.
//!
//! Key details:
//! - This first backend increment supports straight-line main blocks and reports
//!   explicit unsupported-feature errors for control flow not lowered yet.
//! - The main prologue initializes supported static-property storage before
//!   user blocks run.

use crate::codegen::abi;
use crate::codegen::context::DeferredFiberWrapper;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit_fiber_wrapper;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::Emit;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::ir::{BasicBlock, Function, InstId, Module};
use crate::names::{
    enum_case_symbol, function_epilogue_symbol, function_symbol, method_symbol, php_symbol_key,
    static_method_symbol, static_property_symbol,
};
use crate::parser::ast::ExprKind;
use crate::types::{EnumCaseInfo, EnumCaseValue, FunctionSig, PhpType};

use super::context::FunctionContext;
use super::fibers;
use super::frame;
use super::function_variants;
use super::literal_defaults::{
    emit_array_literal_default_to_result, emit_assoc_array_literal_default_to_result,
    emit_boxed_null_literal_to_result,
    emit_boxed_bool_literal_to_result, emit_boxed_float_literal_to_result,
    emit_boxed_int_literal_to_result, emit_boxed_string_literal_default_to_result,
    emit_empty_assoc_array_literal_to_result,
    emit_string_literal_default_to_result, emit_tagged_null_literal_to_result,
    literal_default_value, LiteralDefaultValue,
};
use super::lower_inst;
use super::lower_term;
use super::{CodegenIrError, Result};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits all supported EIR functions and then the process-entry main function.
pub(super) fn emit_module(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    gc_stats: bool,
    heap_debug: bool,
    requires_elephc_tls: bool,
    emit: Emit,
    regalloc_linear: bool,
) -> Result<()> {
    function_variants::emit_dispatchers(module, emitter, data);
    for function in module.functions.iter().filter(|function| !is_main(function)) {
        emit_user_function(module, function, emitter, data, regalloc_linear)?;
    }
    for method in &module.class_methods {
        emit_class_method(module, method, emitter, data, regalloc_linear)?;
    }
    for closure in &module.closures {
        emit_user_function(module, closure, emitter, data, regalloc_linear)?;
    }
    emit_eir_fiber_wrappers(module, emitter);
    if matches!(emit, Emit::Cdylib) {
        return Ok(());
    }
    let main = module
        .functions
        .iter()
        .find(|function| is_main(function))
        .ok_or_else(|| CodegenIrError::invalid_module("EIR module has no main function"))?;
    emit_main_function(
        module,
        main,
        emitter,
        data,
        gc_stats,
        heap_debug,
        requires_elephc_tls,
        regalloc_linear,
    )
}

/// Emits the static EIR Fiber wrappers needed for closure callbacks.
fn emit_eir_fiber_wrappers(module: &Module, emitter: &mut Emitter) {
    for wrapper in required_eir_fiber_wrappers(module) {
        let wrapper = DeferredFiberWrapper {
            label: wrapper.label,
            sig: wrapper.sig,
            visible_param_count: wrapper.visible_param_count,
            hidden_arg_types: wrapper.hidden_arg_types,
            retain_hidden_args_for_closure_call: false,
            use_descriptor_invoker: wrapper.use_descriptor_invoker,
        };
        emit_fiber_wrapper(emitter, &wrapper);
    }
}

/// Collects unique Fiber wrappers needed by this module.
fn required_eir_fiber_wrappers(module: &Module) -> Vec<fibers::FiberWrapper> {
    let mut wrappers = Vec::new();
    for function in all_module_functions(module) {
        for inst in &function.instructions {
            let Some(wrapper) = fibers::wrapper_for_fiber_new(module, function, inst) else {
                continue;
            };
            if wrappers
                .iter()
                .any(|existing: &fibers::FiberWrapper| existing.label == wrapper.label)
            {
                continue;
            }
            wrappers.push(wrapper);
        }
    }
    wrappers
}

/// Iterates every function-like body owned by the EIR module.
fn all_module_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
}

/// Emits a non-main EIR function as a direct-call target.
fn emit_user_function(
    module: &Module,
    function: &Function,
    emitter: &mut Emitter,
    data: &mut DataSection,
    regalloc_linear: bool,
) -> Result<()> {
    if function.flags.is_generator {
        let entry_label = user_function_entry_symbol(function);
        return emit_generator_function(module, function, &entry_label, emitter, data);
    }
    let layout = frame::layout_for_function(function, emitter.target, regalloc_linear);
    let epilogue_label = user_function_epilogue_symbol(function);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        layout,
        false,
        false,
        false,
        Some(epilogue_label),
    );
    let entry_label = user_function_entry_symbol(function);
    frame::emit_function_prologue_with_label(&mut ctx, &entry_label)?;
    emit_blocks(&mut ctx)?;
    frame::emit_function_epilogue(&mut ctx);
    Ok(())
}

/// Returns the assembly entry label for a user or synthetic EIR function.
fn user_function_entry_symbol(function: &Function) -> String {
    if is_property_init_thunk(function) {
        return function.name.clone();
    }
    function_symbol(&function.name)
}

/// Returns the epilogue label paired with `user_function_entry_symbol()`.
fn user_function_epilogue_symbol(function: &Function) -> String {
    if is_property_init_thunk(function) {
        return format!("{}_epilogue", function.name);
    }
    function_epilogue_symbol(&function.name)
}

/// Returns true for synthetic property-default init thunks referenced by runtime metadata.
fn is_property_init_thunk(function: &Function) -> bool {
    function.name.starts_with("_class_propinit_")
}

/// Emits a class method using the legacy runtime metadata symbol shape.
fn emit_class_method(
    module: &Module,
    function: &Function,
    emitter: &mut Emitter,
    data: &mut DataSection,
    regalloc_linear: bool,
) -> Result<()> {
    let entry_label = class_method_entry_symbol(function)?;
    if function.flags.is_generator {
        return emit_generator_function(module, function, &entry_label, emitter, data);
    }
    let layout = frame::layout_for_function(function, emitter.target, regalloc_linear);
    let epilogue_label = format!("{}_epilogue", entry_label);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        layout,
        false,
        false,
        false,
        Some(epilogue_label),
    );
    frame::emit_function_prologue_with_label(&mut ctx, &entry_label)?;
    emit_blocks(&mut ctx)?;
    frame::emit_function_epilogue(&mut ctx);
    Ok(())
}

/// Emits a generator wrapper/resume pair from source metadata carried by the EIR function.
fn emit_generator_function(
    module: &Module,
    function: &Function,
    entry_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) -> Result<()> {
    let source = function.generator_source.as_ref().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "generator function '{}' is missing retained source metadata",
            function.name
        ))
    })?;
    let signature = generator_signature(function, source.visible_param_count);
    let hidden_params = generator_hidden_params(function, source.visible_param_count);
    crate::codegen::emit_generator_with_label(
        emitter,
        data,
        entry_label,
        &signature,
        &hidden_params,
        &source.body,
        Some(&module.class_infos),
    );
    Ok(())
}

/// Rebuilds the visible generator signature from EIR parameter metadata.
fn generator_signature(function: &Function, visible_param_count: usize) -> FunctionSig {
    FunctionSig {
        params: function
            .params
            .iter()
            .take(visible_param_count)
            .map(|param| (param.name.clone(), param.php_type.clone()))
            .collect(),
        defaults: function
            .params
            .iter()
            .take(visible_param_count)
            .map(|_| None)
            .collect(),
        return_type: function.return_php_type.clone(),
        declared_return: !matches!(function.return_php_type, PhpType::Mixed),
        ref_params: function
            .params
            .iter()
            .take(visible_param_count)
            .map(|param| param.by_ref)
            .collect(),
        declared_params: function
            .params
            .iter()
            .take(visible_param_count)
            .map(|param| !matches!(param.php_type, PhpType::Mixed))
            .collect(),
        variadic: function
            .params
            .iter()
            .take(visible_param_count)
            .find(|param| param.variadic)
            .map(|param| param.name.clone()),
        deprecation: None,
    }
}

/// Returns hidden generator parameters such as closure captures.
fn generator_hidden_params(
    function: &Function,
    visible_param_count: usize,
) -> Vec<(String, PhpType, bool)> {
    function
        .params
        .iter()
        .skip(visible_param_count)
        .map(|param| (param.name.clone(), param.php_type.clone(), param.by_ref))
        .collect()
}

/// Returns the runtime metadata entry label for an EIR class-method function.
fn class_method_entry_symbol(function: &Function) -> Result<String> {
    let Some((class_name, method_name)) = function.name.rsplit_once("::") else {
        return Err(CodegenIrError::invalid_module(format!(
            "class method function '{}' has no class receiver",
            function.name
        )));
    };
    let method_key = php_symbol_key(method_name);
    if function.flags.is_static {
        Ok(static_method_symbol(class_name, &method_key))
    } else {
        Ok(method_symbol(class_name, &method_key))
    }
}

/// Emits the EIR main function as the process entry point.
fn emit_main_function(
    module: &Module,
    function: &Function,
    emitter: &mut Emitter,
    data: &mut DataSection,
    gc_stats: bool,
    heap_debug: bool,
    requires_elephc_tls: bool,
    regalloc_linear: bool,
) -> Result<()> {
    let layout = frame::layout_for_function(function, emitter.target, regalloc_linear);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        layout,
        true,
        gc_stats,
        heap_debug,
        None,
    );
    frame::emit_main_prologue(&mut ctx);
    if requires_elephc_tls {
        crate::codegen::builtins::publish_tls_function_pointers(ctx.emitter);
    }
    emit_enum_singleton_initializers(&mut ctx);
    emit_static_property_initializers(&mut ctx)?;
    emit_blocks(&mut ctx)?;
    if !ctx.epilogue_emitted {
        frame::emit_main_epilogue(&mut ctx);
    }
    Ok(())
}

/// Returns true when a function is the process entry function.
fn is_main(function: &Function) -> bool {
    function.flags.is_main || function.name == "main"
}

/// Emits global singleton objects for enum cases used by EIR user code.
fn emit_enum_singleton_initializers(ctx: &mut FunctionContext<'_>) {
    let allowed_class_names = super::runtime_referenced_class_names(ctx.module);
    let mut sorted_enums = ctx.module.enum_infos.iter().collect::<Vec<_>>();
    sorted_enums.sort_by_key(|(name, _)| name.as_str());
    for (enum_name, enum_info) in sorted_enums {
        if !allowed_class_names.contains(enum_name) {
            continue;
        }
        let Some(class_info) = ctx.module.class_infos.get(enum_name) else {
            continue;
        };
        for case in &enum_info.cases {
            emit_enum_singleton_initializer(
                ctx,
                enum_name,
                class_info.class_id,
                class_info.properties.len(),
                case,
            );
        }
    }
}

/// Emits one enum case singleton allocation and publishes it to its global slot.
fn emit_enum_singleton_initializer(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    class_id: u64,
    property_count: usize,
    case: &EnumCaseInfo,
) {
    ctx.emitter.comment(&format!("initialize enum singleton {}::{}", enum_name, case.name));
    emit_enum_object_allocation(ctx, class_id, property_count);
    if let Some(case_value) = &case.value {
        emit_enum_backing_value(ctx, case_value);
    }
    let symbol = enum_case_symbol(enum_name, &case.name);
    abi::emit_store_reg_to_symbol(ctx.emitter, abi::int_result_reg(ctx.emitter), &symbol, 0);
}

/// Allocates an object-shaped enum singleton and zeroes its property storage.
fn emit_enum_object_allocation(ctx: &mut FunctionContext<'_>, class_id: u64, property_count: usize) {
    let payload_size = 8 + property_count * 16;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x0, #{}", payload_size));     // request enum singleton object payload storage
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #4");                              // heap kind 4 marks enum singletons as object instances
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the heap header before the enum singleton payload
            ctx.emitter.instruction(&format!("mov x10, #{}", class_id));        // materialize the enum class id
            ctx.emitter.instruction("str x10, [x0]");                           // store the enum class id at payload offset zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rax, {}", payload_size));     // request enum singleton object payload storage
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the x86_64 object heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the heap header before the enum singleton payload
            ctx.emitter.instruction(&format!("mov r10, {}", class_id));         // materialize the enum class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the enum class id at payload offset zero
        }
    }
    let object_reg = abi::int_result_reg(ctx.emitter);
    for index in 0..property_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset);
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset + 8);
    }
}

/// Writes a backed enum case value into the singleton's first property slot.
fn emit_enum_backing_value(ctx: &mut FunctionContext<'_>, case_value: &EnumCaseValue) {
    let object_reg = abi::int_result_reg(ctx.emitter);
    let temp_reg = abi::temp_int_reg(ctx.emitter.target);
    match case_value {
        EnumCaseValue::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, temp_reg, *value);
            abi::emit_store_to_address(ctx.emitter, temp_reg, object_reg, 8);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, 16);
        }
        EnumCaseValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (label, len) = ctx.data.add_string(&bytes);
            abi::emit_symbol_address(ctx.emitter, temp_reg, &label);
            abi::emit_store_to_address(ctx.emitter, temp_reg, object_reg, 8);
            abi::emit_load_int_immediate(ctx.emitter, temp_reg, len as i64);
            abi::emit_store_to_address(ctx.emitter, temp_reg, object_reg, 16);
        }
    }
}

/// Initializes static-property storage before user code runs.
fn emit_static_property_initializers(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let mut default_initializers = Vec::new();
    let mut uninitialized_static_properties = Vec::new();
    let mut class_names = super::runtime_referenced_class_names(ctx.module)
        .into_iter()
        .collect::<Vec<_>>();
    class_names.sort();
    for class_name in class_names {
        let Some(class_info) = ctx.module.class_infos.get(&class_name) else {
            continue;
        };
        for (index, (property, php_type)) in class_info.static_properties.iter().enumerate() {
            let declaring_class = class_info
                .static_property_declaring_classes
                .get(property)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            if declaring_class != class_name {
                continue;
            }
            let default = class_info.static_defaults.get(index).and_then(Option::as_ref);
            if let Some(default_expr) = default {
                default_initializers.push((
                    class_name.clone(),
                    property.clone(),
                    php_type.clone(),
                    default_expr.kind.clone(),
                ));
            } else if class_info.declared_static_properties.contains(property) {
                uninitialized_static_properties.push((class_name.clone(), property.clone()));
            }
        }
    }
    for (class_name, property) in uninitialized_static_properties {
        emit_static_property_sentinel(ctx, &class_name, &property);
    }
    for (class_name, property, php_type, expr) in default_initializers {
        emit_static_property_default(ctx, &class_name, &property, &php_type, &expr)?;
    }
    Ok(())
}

/// Marks one typed static property without a default as uninitialized.
fn emit_static_property_sentinel(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
) {
    ctx.emitter.comment(&format!(
        "mark static property {}::${} uninitialized",
        class_name, property
    ));
    let marker_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(
        ctx.emitter,
        marker_reg,
        UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
    );
    let symbol = static_property_symbol(class_name, property);
    abi::emit_store_reg_to_symbol(ctx.emitter, marker_reg, &symbol, 8);
}

/// Writes a supported literal static-property default into its symbol storage.
fn emit_static_property_default(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
    php_type: &PhpType,
    expr: &ExprKind,
) -> Result<()> {
    ensure_static_property_default_type_supported(class_name, property, php_type)?;
    let value = literal_default_value(
        &format!("static property {}::${}", class_name, property),
        php_type,
        expr,
        "static property initializer",
    )?;
    ctx.emitter.comment(&format!(
        "initialize static property {}::${}",
        class_name, property
    ));
    emit_static_property_default_value(ctx, class_name, property, php_type, &value)?;
    Ok(())
}

/// Verifies the EIR static-property initializer has a direct storage representation.
fn ensure_static_property_default_type_supported(
    class_name: &str,
    property: &str,
    php_type: &PhpType,
) -> Result<()> {
    match php_type {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Void
        | PhpType::Never
        | PhpType::Mixed
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Union(_) => Ok(()),
        _ => Err(CodegenIrError::unsupported(format!(
            "static property initializer for {}::${} with PHP type {:?}",
            class_name, property, php_type
        ))),
    }
}

/// Emits the target-specific literal load and symbol store for one static-property default.
fn emit_static_property_default_value(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
    php_type: &PhpType,
    value: &LiteralDefaultValue,
) -> Result<()> {
    match value {
        LiteralDefaultValue::Int(value) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, *value);
        }
        LiteralDefaultValue::Bool(value) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, i64::from(*value));
        }
        LiteralDefaultValue::Float(value) => {
            let label = ctx.data.add_float(*value);
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_load_symbol_to_reg(ctx.emitter, float_reg, &label, 0);
        }
        LiteralDefaultValue::Str(value) => {
            emit_string_literal_default_to_result(ctx, value);
        }
        LiteralDefaultValue::Null => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        LiteralDefaultValue::NullSentinel => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
        }
        LiteralDefaultValue::TaggedNull => {
            emit_tagged_null_literal_to_result(ctx);
        }
        LiteralDefaultValue::BoxedNull => {
            emit_boxed_null_literal_to_result(ctx);
        }
        LiteralDefaultValue::BoxedStr(value) => {
            emit_boxed_string_literal_default_to_result(ctx, value);
        }
        LiteralDefaultValue::BoxedInt(value) => {
            emit_boxed_int_literal_to_result(ctx, *value);
        }
        LiteralDefaultValue::BoxedBool(value) => {
            emit_boxed_bool_literal_to_result(ctx, *value);
        }
        LiteralDefaultValue::BoxedFloat(value) => {
            emit_boxed_float_literal_to_result(ctx, *value);
        }
        LiteralDefaultValue::Array {
            elem_type,
            elements,
        } => {
            emit_array_literal_default_to_result(ctx, elem_type, elements)?;
        }
        LiteralDefaultValue::AssocArray {
            value_type,
            elements,
        } => {
            emit_assoc_array_literal_default_to_result(ctx, value_type, elements)?;
        }
        LiteralDefaultValue::EmptyAssocArray { value_type } => {
            emit_empty_assoc_array_literal_to_result(ctx, value_type);
        }
    }
    let symbol = static_property_symbol(class_name, property);
    abi::emit_store_result_to_symbol(ctx.emitter, &symbol, php_type, false);
    if !matches!(php_type.codegen_repr(), PhpType::Str | PhpType::TaggedScalar) {
        abi::emit_store_zero_to_symbol(ctx.emitter, &symbol, 8);
    }
    Ok(())
}

/// Emits every block in table order.
fn emit_blocks(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let blocks = ctx.function.blocks.clone();
    for block in blocks {
        emit_block(ctx, &block)?;
    }
    Ok(())
}

/// Emits one EIR basic block.
fn emit_block(ctx: &mut FunctionContext<'_>, block: &BasicBlock) -> Result<()> {
    ctx.emitter.label(&ctx.block_label(&block.name, block.id.as_raw()));
    for inst_id in &block.instructions {
        emit_instruction_source_marker(ctx, *inst_id)?;
        lower_inst::lower_instruction(ctx, *inst_id)?;
    }
    let terminator = block
        .terminator
        .as_ref()
        .ok_or_else(|| CodegenIrError::invalid_module(format!("block '{}' has no terminator", block.name)))?;
    lower_term::lower_terminator(ctx, terminator)
}

/// Emits the source-map marker for an EIR instruction when it carries a real PHP span.
fn emit_instruction_source_marker(ctx: &mut FunctionContext<'_>, inst_id: InstId) -> Result<()> {
    let Some(inst) = ctx.function.instruction(inst_id) else {
        return Err(CodegenIrError::missing_entry("instruction", inst_id.as_raw()));
    };
    let Some(span) = inst.span else {
        return Ok(());
    };
    if span.line > 0 {
        ctx.emitter
            .comment(&format!("@src line={} col={}", span.line, span.col));
    }
    Ok(())
}
