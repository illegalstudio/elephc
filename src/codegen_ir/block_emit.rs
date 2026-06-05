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
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::ir::{BasicBlock, Function, Module};
use crate::names::{
    enum_case_symbol, function_epilogue_symbol, method_symbol, php_symbol_key, static_method_symbol,
    static_property_symbol,
};
use crate::parser::ast::ExprKind;
use crate::types::{EnumCaseInfo, EnumCaseValue, PhpType};

use super::context::FunctionContext;
use super::frame;
use super::function_variants;
use super::literal_defaults::{
    emit_array_literal_default_to_result, literal_default_value, LiteralDefaultValue,
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
) -> Result<()> {
    function_variants::emit_dispatchers(module, emitter, data);
    for function in module.functions.iter().filter(|function| !is_main(function)) {
        emit_user_function(module, function, emitter, data)?;
    }
    for method in &module.class_methods {
        emit_class_method(module, method, emitter, data)?;
    }
    for closure in &module.closures {
        emit_user_function(module, closure, emitter, data)?;
    }
    let main = module
        .functions
        .iter()
        .find(|function| is_main(function))
        .ok_or_else(|| CodegenIrError::invalid_module("EIR module has no main function"))?;
    emit_main_function(module, main, emitter, data)
}

/// Emits a non-main EIR function as a direct-call target.
fn emit_user_function(
    module: &Module,
    function: &Function,
    emitter: &mut Emitter,
    data: &mut DataSection,
) -> Result<()> {
    let layout = frame::layout_for_function(function);
    let epilogue_label = function_epilogue_symbol(&function.name);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        layout,
        false,
        Some(epilogue_label),
    );
    frame::emit_function_prologue(&mut ctx)?;
    emit_blocks(&mut ctx)?;
    frame::emit_function_epilogue(&mut ctx);
    Ok(())
}

/// Emits a class method using the legacy runtime metadata symbol shape.
fn emit_class_method(
    module: &Module,
    function: &Function,
    emitter: &mut Emitter,
    data: &mut DataSection,
) -> Result<()> {
    let layout = frame::layout_for_function(function);
    let entry_label = class_method_entry_symbol(function)?;
    let epilogue_label = format!("{}_epilogue", entry_label);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        layout,
        false,
        Some(epilogue_label),
    );
    frame::emit_function_prologue_with_label(&mut ctx, &entry_label)?;
    emit_blocks(&mut ctx)?;
    frame::emit_function_epilogue(&mut ctx);
    Ok(())
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
) -> Result<()> {
    let layout = frame::layout_for_function(function);
    let mut ctx = FunctionContext::new(module, function, emitter, data, layout, true, None);
    frame::emit_main_prologue(&mut ctx);
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
        PhpType::Bool | PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Array(_) => Ok(()),
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
            let (label, len) = ctx.data.add_string(value.as_bytes());
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
        }
        LiteralDefaultValue::Array {
            elem_type,
            elements,
        } => {
            emit_array_literal_default_to_result(ctx, elem_type, elements)?;
        }
    }
    let symbol = static_property_symbol(class_name, property);
    abi::emit_store_result_to_symbol(ctx.emitter, &symbol, php_type, false);
    if !matches!(php_type.codegen_repr(), PhpType::Str) {
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
        lower_inst::lower_instruction(ctx, *inst_id)?;
    }
    let terminator = block
        .terminator
        .as_ref()
        .ok_or_else(|| CodegenIrError::invalid_module(format!("block '{}' has no terminator", block.name)))?;
    lower_term::lower_terminator(ctx, terminator)
}
