//! Purpose:
//! Lowers typed internal-extension EIR calls through their versioned native bridge ABI.
//! Keeps opcode metadata target-neutral until the final assembly materialization layer.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` for `InternalExtensionCall`.
//!
//! Key details:
//! - Flat requests preserve embedded NUL bytes and never expose Rust/native layouts to PHP objects.
//! - Native result frames are copied or adopted before their independent result IDs are released.
//! - SimpleXML iterator moves transfer their private eager result into a strong parent-owned slot.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{
    callable_descriptor, callable_dispatch, emit_box_current_owned_value_as_mixed,
    emit_box_current_value_as_mixed, CodegenIrError, Result,
};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::ir::{Immediate, Instruction, ValueId};
use crate::types::PhpType;

use super::expect_operand;

mod result_tree;

const FLAG_RECEIVER: u32 = 1;
const FLAG_WRAPPER_RESULT: u32 = 2;
const FLAG_VALUE_OBJECT_RESULT: u32 = 4;
const FLAG_ARRAY_APPEND_OFFSET: u32 = 8;
const ABI_VERSION: i64 = 1;
const REQUEST_HEADER_SIZE: usize = 48;
const ABI_VALUE_SIZE: usize = 24;
const ABI_DIAGNOSTIC_SIZE: usize = 64;
const RESULT_FRAME_OFFSET: usize = 48;
const RESULT_HEADER_SIZE: usize = 96;
const TEMP_RESULT_LO_OFFSET: usize = 144;
const TEMP_RESULT_HI_OFFSET: usize = 152;
const ARRAY_RESULT_OFFSET: usize = 160;
const ARRAY_INDEX_OFFSET: usize = 168;
const ARRAY_COUNT_OFFSET: usize = 176;
const ARRAY_VALUE_OFFSET: usize = 184;
const OBJECT_RESULT_OFFSET: usize = 192;
const OBJECT_FIELDS_OFFSET: usize = 200;
const OBJECT_FIELD_COUNT_OFFSET: usize = 208;
const TEMP_CALLABLE_DESCRIPTOR_OFFSET: usize = 216;
const RUNTIME_VALUE_MEASURE_OFFSET: usize = 224;
const TOTAL_VALUE_COUNT_OFFSET: usize = 240;
const TOTAL_BYTE_COUNT_OFFSET: usize = 248;
const RUNTIME_VALUE_OFFSET: usize = 256;
const RUNTIME_VALUE_WRITE_CONTEXT_OFFSET: usize = 280;
const RUNTIME_VALUE_POINTER_OFFSET: usize = 352;
const PREPARED_XPATH_CALLBACK_VALUE_OFFSET: usize = 360;
const RECEIVER_OBJECT_OFFSET: usize = 368;
const RECEIVER_ITERATOR_EPOCH_OFFSET: usize = 376;
const CALL_FRAME_SIZE: usize = 384;
const LEGACY_DOM_APPEND_CHILD_OPCODE: u32 = 4386;
const MODERN_DOM_XPATH_QUERY_OPCODE: u32 = 4281;
const LEGACY_DOM_XPATH_QUERY_OPCODE: u32 = 4420;
const SIMPLEXML_CONSTRUCTOR_OPCODE: u32 = 4425;
const SIMPLEXML_ADD_ATTRIBUTE_OPCODE: u32 = 4428;
const SIMPLEXML_ADD_CHILD_OPCODE: u32 = 4429;
const SIMPLEXML_READ_DIMENSION_OPCODE: u32 = 4453;
const SIMPLEXML_WRITE_DIMENSION_OPCODE: u32 = 4457;
const SIMPLEXML_NEXT_OPCODE: u32 = 4441;
const SIMPLEXML_REWIND_OPCODE: u32 = 4443;
const SIMPLEXML_XPATH_OPCODE: u32 = 4446;
const DIAGNOSTIC_FLAG_CALLSITE_CONTEXT: i64 = 1;
const DIAGNOSTIC_FLAG_CALLSITE_LOCATION: i64 = 1 << 1;
const REQUEST_FLAG_ARGUMENT_COUNT: i64 = 1_i64 << 31;
const RUNTIME_VALUE_MAX_DEPTH: i64 = 2;
const DOM_XPATH_CALLABLE_RESOLVER_FRAME_SIZE: usize = 32;
const PREPARED_STRING_SLOT_SIZE: usize = 32;
const PREPARED_STRING_TAG_OFFSET: usize = 16;
const PREPARED_MIXED_VALUE_OFFSET: usize = 24;

/// One object argument coerced through `__toString()` before request sizing.
#[derive(Clone, Copy)]
struct PreparedStringArgument {
    argument_index: usize,
    stack_offset: usize,
    runtime_polymorphic: bool,
}

/// PHP-visible contract for one runtime-polymorphic native-wrapper parameter.
struct NativeWrapperParameterContract {
    callable: String,
    property: Option<String>,
    position: usize,
    name: Option<String>,
    expected_type: String,
    allows_null: bool,
    allows_string: bool,
    allows_stringable: bool,
    variadic: bool,
    wrapper_bases: Vec<String>,
}

/// Lowers one typed internal-extension operation through the locked flat native ABI.
pub(super) fn lower_internal_extension_call(
    ctx: &mut FunctionContext<'_>,
    instruction: &Instruction,
) -> Result<()> {
    let Some(Immediate::InternalExtension { opcode, flags }) = instruction.immediate else {
        return Err(CodegenIrError::invalid_module(
            "internal_extension_call requires opcode metadata",
        ));
    };
    lower_internal_extension_call_with_receiver(
        ctx,
        instruction,
        opcode,
        flags,
        None,
        &instruction.result_php_type,
    )
}

/// Lowers a native-wrapper operation selected from a boxed `Mixed` receiver.
///
/// The generic mixed dispatcher has already unboxed and class-checked the
/// receiver before reaching this path. Reusing the typed bridge request encoder
/// avoids routing virtual extension members through ordinary PHP object slots.
pub(super) fn lower_mixed_receiver_internal_extension_call(
    ctx: &mut FunctionContext<'_>,
    instruction: &Instruction,
    receiver_reg: &str,
    opcode: u32,
    result_contract: &PhpType,
) -> Result<()> {
    let flags = internal_extension_result_flags(result_contract);
    lower_internal_extension_call_with_receiver(
        ctx,
        instruction,
        opcode,
        flags,
        Some(receiver_reg),
        result_contract,
    )
}

/// Runs the common flat-ABI request path with either an EIR receiver or a pre-unboxed register.
fn lower_internal_extension_call_with_receiver(
    ctx: &mut FunctionContext<'_>,
    instruction: &Instruction,
    opcode: u32,
    flags: u32,
    unboxed_receiver_reg: Option<&str>,
    result_contract: &PhpType,
) -> Result<()> {
    let has_receiver = flags & FLAG_RECEIVER != 0;
    if unboxed_receiver_reg.is_some() && !has_receiver {
        return Err(CodegenIrError::invalid_module(
            "mixed internal-extension method call requires a receiver flag",
        ));
    }
    let argument_start = usize::from(has_receiver);
    let arguments = &instruction.operands[argument_start..];
    let saved_unboxed_receiver = unboxed_receiver_reg.map(str::to_string);
    if let Some(receiver_reg) = saved_unboxed_receiver.as_deref() {
        abi::emit_push_reg(ctx.emitter, receiver_reg);
    }

    emit_dom_xpath_callable_resolver(ctx)?;
    let (prepared_strings, terminates_during_preflight) =
        prepare_stringable_arguments(ctx, arguments, opcode)?;
    if terminates_during_preflight {
        return Ok(());
    }
    abi::emit_reserve_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
    store_stack_immediate(ctx, TEMP_CALLABLE_DESCRIPTOR_OFFSET, 0);
    store_stack_immediate(ctx, PREPARED_XPATH_CALLBACK_VALUE_OFFSET, 0);
    if let Some(receiver_reg) = saved_unboxed_receiver.as_deref() {
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            receiver_reg,
            CALL_FRAME_SIZE + prepared_string_stack_bytes(&prepared_strings),
        );
        emit_unboxed_receiver_metadata(ctx, receiver_reg);
    } else if has_receiver {
        let receiver = expect_operand(instruction, 0)?;
        emit_receiver_metadata(ctx, receiver)?;
    } else {
        abi::emit_call_label(ctx.emitter, "__rt_dom_context_ensure");
        abi::emit_store_to_sp(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            16,
        );
        store_stack_immediate(ctx, 24, 0);
    }

    emit_request_size(ctx, arguments, opcode, &prepared_strings)?;
    allocate_request(ctx)?;
    emit_request_header(ctx, opcode, arguments.len())?;
    emit_runtime_value_write_context(ctx, arguments.len())?;
    for (index, argument) in arguments.iter().copied().enumerate() {
        emit_request_value(
            ctx,
            argument,
            index,
            opcode,
            flags,
            &prepared_strings,
        )?;
    }
    release_prepared_string_values(ctx, &prepared_strings);
    let reentrant_move_labels = simplexml_iterator_move_opcode(opcode).then(|| {
        (
            ctx.next_label("simplexml_iterator_move_reentered"),
            ctx.next_label("simplexml_iterator_move_complete"),
        )
    });
    if let Some((reentered, _)) = &reentrant_move_labels {
        emit_snapshot_simplexml_iterator_epoch(ctx);
        emit_clear_simplexml_iterator_current_owner(ctx);
        emit_branch_if_simplexml_iterator_epoch_changed(ctx, reentered);
    }
    emit_native_call(ctx, instruction, opcode)?;
    emit_result(ctx, instruction, opcode, flags, result_contract)?;
    if let Some((reentered, complete)) = &reentrant_move_labels {
        abi::emit_jump(ctx.emitter, complete);
        ctx.emitter.label(reentered);
        emit_release_abandoned_native_request_state(ctx);
        abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
        if instruction.result.is_some() {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                crate::codegen::NULL_SENTINEL,
            );
            store_instruction_result(ctx, instruction)?;
        }
        ctx.emitter.label(complete);
    }
    abi::emit_release_temporary_stack(
        ctx.emitter,
        prepared_string_stack_bytes(&prepared_strings),
    );
    if saved_unboxed_receiver.is_some() {
        abi::emit_pop_reg(ctx.emitter, abi::temp_int_reg(ctx.emitter.target));
    }
    Ok(())
}

/// Prepares strings and validates dynamic wrapper unions in PHP argument order.
fn prepare_stringable_arguments(
    ctx: &mut FunctionContext<'_>,
    arguments: &[ValueId],
    opcode: u32,
) -> Result<(Vec<PreparedStringArgument>, bool)> {
    let mut prepared_strings = Vec::new();
    let mut terminates = false;
    for (index, argument) in arguments.iter().copied().enumerate() {
        let raw_type = ctx.raw_value_php_type(argument)?;
        let Some(contract) = native_wrapper_parameter_contract(opcode, index) else {
            continue;
        };
        if let PhpType::Object(class_name) = raw_type.codegen_repr() {
            if contract.allows_stringable
                && !static_object_matches_wrapper_contract(ctx, &class_name, &contract)
                && super::object_class_has_tostring(
                    ctx,
                    class_name.trim_start_matches('\\'),
                )
            {
                prepared_strings.push(PreparedStringArgument {
                    argument_index: index,
                    stack_offset: CALL_FRAME_SIZE
                        + prepared_strings.len() * PREPARED_STRING_SLOT_SIZE,
                    runtime_polymorphic: false,
                });
            }
        }
        if raw_type.codegen_repr() == PhpType::Mixed {
            prepared_strings.push(PreparedStringArgument {
                argument_index: index,
                stack_offset: CALL_FRAME_SIZE
                    + prepared_strings.len() * PREPARED_STRING_SLOT_SIZE,
                runtime_polymorphic: true,
            });
        }
    }

    let stack_bytes = prepared_string_stack_bytes(&prepared_strings);
    if stack_bytes != 0 {
        abi::emit_reserve_temporary_stack(ctx.emitter, stack_bytes);
        for prepared in &prepared_strings {
            let offset = prepared.stack_offset - CALL_FRAME_SIZE;
            store_stack_immediate(ctx, offset, 0);
            store_stack_immediate(ctx, offset + 8, 0);
            store_stack_immediate(ctx, offset + PREPARED_STRING_TAG_OFFSET, 0);
            if prepared.runtime_polymorphic {
                ctx.load_value_to_result(arguments[prepared.argument_index])?;
                abi::emit_store_to_sp(
                    ctx.emitter,
                    abi::int_result_reg(ctx.emitter),
                    offset + PREPARED_MIXED_VALUE_OFFSET,
                );
            } else {
                store_stack_immediate(ctx, offset + PREPARED_MIXED_VALUE_OFFSET, 0);
            }
        }
        abi::emit_reserve_temporary_stack(ctx.emitter, TRY_HANDLER_SLOT_SIZE);
    }

    let escape = ctx.next_label("dom_stringable_argument_escape");
    let ready = ctx.next_label("dom_stringable_arguments_ready");
    if stack_bytes != 0 {
        emit_prepared_string_exception_boundary_push(ctx, &escape);
    }

    for (index, argument) in arguments.iter().copied().enumerate() {
        let raw_type = ctx.raw_value_php_type(argument)?;
        let Some(contract) = native_wrapper_parameter_contract(opcode, index) else {
            continue;
        };
        let prepared = prepared_string_argument(&prepared_strings, index);
        match raw_type.codegen_repr() {
            PhpType::Object(class_name) => {
                if static_object_matches_wrapper_contract(ctx, &class_name, &contract) {
                    continue;
                }
                if contract.allows_stringable
                    && super::object_class_has_tostring(
                        ctx,
                        class_name.trim_start_matches('\\'),
                    )
                {
                    let prepared = prepared.ok_or_else(|| {
                        CodegenIrError::invalid_module(
                            "Stringable internal-extension argument has no staging slot",
                        )
                    })?;
                    let return_type = super::emit_object_tostring_call(
                        ctx,
                        argument,
                        class_name.trim_start_matches('\\'),
                    )?;
                    if return_type.codegen_repr() != PhpType::Str {
                        return Err(CodegenIrError::unsupported(format!(
                            "__toString return value for internal-extension argument type {:?}",
                            return_type,
                        )));
                    }
                    emit_stage_stringable_result(
                        ctx,
                        prepared_preflight_stack_offset(prepared),
                    );
                    continue;
                }
                emit_native_wrapper_type_error(
                    ctx,
                    &contract,
                    class_name.trim_start_matches('\\'),
                );
                terminates = true;
                break;
            }
            PhpType::Mixed => {
                emit_mixed_native_wrapper_contract_validation(
                    ctx,
                    argument,
                    &contract,
                    prepared,
                )?;
            }
            _ => {}
        }
    }

    if stack_bytes != 0 {
        emit_prepared_string_exception_boundary_pop(ctx);
        abi::emit_release_temporary_stack(ctx.emitter, TRY_HANDLER_SLOT_SIZE);
        abi::emit_jump(ctx.emitter, &ready);
        ctx.emitter.label(&escape);
        release_prepared_string_values_at_base(
            ctx,
            &prepared_strings,
            TRY_HANDLER_SLOT_SIZE,
        );
        emit_prepared_string_exception_boundary_pop(ctx);
        abi::emit_release_temporary_stack(
            ctx.emitter,
            TRY_HANDLER_SLOT_SIZE + stack_bytes,
        );
        abi::emit_jump(ctx.emitter, "__rt_throw_current");
        ctx.emitter.label(&ready);
    }
    Ok((prepared_strings, terminates))
}

/// Releases prepared string payloads once every byte range is copied into the request.
fn release_prepared_string_values(
    ctx: &mut FunctionContext<'_>,
    prepared_strings: &[PreparedStringArgument],
) {
    release_prepared_string_values_at_base(ctx, prepared_strings, CALL_FRAME_SIZE);
}

/// Releases staged owned strings at one caller-supplied stack-frame base.
fn release_prepared_string_values_at_base(
    ctx: &mut FunctionContext<'_>,
    prepared_strings: &[PreparedStringArgument],
    base_offset: usize,
) {
    let result = abi::int_result_reg(ctx.emitter).to_string();
    for prepared in prepared_strings {
        let offset = base_offset + prepared.stack_offset - CALL_FRAME_SIZE;
        abi::emit_load_temporary_stack_slot(ctx.emitter, &result, offset);
        abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    }
}

/// Returns the aligned temporary-stack storage required by all staged strings.
fn prepared_string_stack_bytes(prepared_strings: &[PreparedStringArgument]) -> usize {
    prepared_strings.len() * PREPARED_STRING_SLOT_SIZE
}

/// Returns one staged string's offset while the exception boundary is installed.
fn prepared_preflight_stack_offset(prepared: PreparedStringArgument) -> usize {
    TRY_HANDLER_SLOT_SIZE + prepared.stack_offset - CALL_FRAME_SIZE
}

/// Reloads one snapshotted boxed value without consulting call-clobbered SSA placement.
fn emit_load_prepared_mixed_value(
    ctx: &mut FunctionContext<'_>,
    prepared: PreparedStringArgument,
    base_offset: usize,
    destination: &str,
) {
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        destination,
        base_offset + prepared.stack_offset - CALL_FRAME_SIZE + PREPARED_MIXED_VALUE_OFFSET,
    );
}

/// Reports whether one concrete object belongs to an accepted wrapper arm.
fn static_object_matches_wrapper_contract(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    contract: &NativeWrapperParameterContract,
) -> bool {
    contract.wrapper_bases.iter().any(|wrapper_base| {
        super::mixed_internal_extension_class_is_a(ctx, class_name, wrapper_base)
    })
}

/// Persists the current string pair and marks its staging slot as selected.
fn emit_stage_owned_string_result(ctx: &mut FunctionContext<'_>, stack_offset: usize) {
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    emit_stage_stringable_result(ctx, stack_offset);
}

/// Stages an owned `__toString()` return without duplicating its caller-owned payload.
fn emit_stage_stringable_result(ctx: &mut FunctionContext<'_>, stack_offset: usize) {
    let (pointer, length) = abi::string_result_regs(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, pointer, stack_offset);
    abi::emit_store_to_sp(ctx.emitter, length, stack_offset + 8);
    store_stack_immediate(ctx, stack_offset + PREPARED_STRING_TAG_OFFSET, 1);
}

/// Installs a temporary catch boundary around userland `__toString()` calls.
fn emit_prepared_string_exception_boundary_push(
    ctx: &mut FunctionContext<'_>,
    escape_label: &str,
) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, "_exc_handler_top", 0);
    abi::emit_store_to_sp(ctx.emitter, scratch, 0);
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, "_exc_call_frame_top", 0);
    abi::emit_store_to_sp(ctx.emitter, scratch, 8);
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, "_rt_diag_suppression", 0);
    abi::emit_store_to_sp(
        ctx.emitter,
        scratch,
        TRY_HANDLER_DIAG_DEPTH_OFFSET,
    );
    abi::emit_temporary_stack_address(ctx.emitter, scratch, 0);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_exc_handler_top", 0);
    abi::emit_temporary_stack_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        TRY_HANDLER_JMP_BUF_OFFSET,
    );
    ctx.emitter.bl_c("setjmp");
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, escape_label);
}

/// Restores the outer exception handler and diagnostic-suppression depth.
fn emit_prepared_string_exception_boundary_pop(ctx: &mut FunctionContext<'_>) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, 0);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_exc_handler_top", 0);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        scratch,
        TRY_HANDLER_DIAG_DEPTH_OFFSET,
    );
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_rt_diag_suppression", 0);
}

/// Returns one staged string argument, including its ownership contract.
fn prepared_string_argument(
    prepared_strings: &[PreparedStringArgument],
    argument_index: usize,
) -> Option<PreparedStringArgument> {
    prepared_strings
        .iter()
        .find(|prepared| prepared.argument_index == argument_index)
        .copied()
}

/// Computes native bridge result-materialization flags for a method-call result type.
///
/// `Mixed` dispatch can retain a precise object or union result even though its
/// receiver is boxed. Native wrapper and value-object members still need their
/// regular bridge materializers before the result is stored in that EIR type.
fn internal_extension_result_flags(result_type: &PhpType) -> u32 {
    let mut flags = FLAG_RECEIVER;
    let members: Vec<&PhpType> = match result_type {
        PhpType::Union(members) => members.iter().collect(),
        member => vec![member],
    };
    for member in members {
        let PhpType::Object(class_name) = member.codegen_repr() else {
            continue;
        };
        if crate::internal_extensions::is_native_wrapper_class(&class_name) {
            flags |= FLAG_WRAPPER_RESULT;
        }
        if crate::internal_extensions::is_native_value_object_class(&class_name) {
            flags |= FLAG_VALUE_OBJECT_RESULT;
        }
    }
    flags
}

/// Emits one module-wide case-insensitive PHP callable-name to descriptor resolver.
pub(super) fn emit_dom_xpath_callable_resolver(ctx: &mut FunctionContext<'_>) -> Result<()> {
    if !ctx.shared.reserve_dom_xpath_callable_resolver() {
        return Ok(());
    }
    let resume = ctx.next_label("dom_xpath_callable_resolver_resume");
    abi::emit_jump(ctx.emitter, &resume);
    let cases =
        super::callables::runtime_string_descriptor_cases(ctx, None, None, false)?;
    ctx.emitter.blank();
    ctx.emitter
        .comment("--- runtime: DOM XPath callable-name resolver ---");
    ctx.emitter
        .label_global("__rt_dom_xpath_resolve_callable");
    abi::emit_frame_prologue(ctx.emitter, DOM_XPATH_CALLABLE_RESOLVER_FRAME_SIZE);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_store_to_sp(ctx.emitter, "x0", 0);
            abi::emit_store_to_sp(ctx.emitter, "x1", 8);
        }
        Arch::X86_64 => {
            abi::emit_store_to_sp(ctx.emitter, "rdi", 0);
            abi::emit_store_to_sp(ctx.emitter, "rsi", 8);
        }
    }
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    let selector = callable_dispatch::RuntimeCallableSelector::StringNameStack {
        ptr_offset: 0,
        len_offset: 8,
        call_reg: &result_reg,
    };
    let done = ctx.next_label("dom_xpath_callable_resolver_done");
    for (index, case) in cases.iter().enumerate() {
        let next = ctx.next_label(&format!("dom_xpath_callable_resolver_next_{index}"));
        let matched =
            ctx.next_label(&format!("dom_xpath_callable_resolver_match_{index}"));
        callable_dispatch::emit_branch_if_callable_case_mismatch(
            &selector,
            case,
            &next,
            ctx.emitter,
            &matched,
            ctx.data,
        );
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&next);
    }
    abi::emit_load_int_immediate(ctx.emitter, &result_reg, 0);
    ctx.emitter.label(&done);
    abi::emit_frame_restore(ctx.emitter, DOM_XPATH_CALLABLE_RESOLVER_FRAME_SIZE);
    abi::emit_return(ctx.emitter);
    super::callables::emit_dom_xpath_callable_array_resolver(ctx)?;
    ctx.emitter.label(&resume);
    Ok(())
}

/// Extracts the context and generation-checked native handle from a receiver wrapper.
fn emit_receiver_metadata(ctx: &mut FunctionContext<'_>, receiver: ValueId) -> Result<()> {
    emit_wrapper_object_pointer(ctx, receiver)?;
    emit_unboxed_receiver_metadata(ctx, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Extracts bridge context and handle metadata from an already unboxed wrapper object register.
///
/// Mixed-method class dispatch has proved both the boxed payload tag and the
/// runtime class before calling this helper, so the register holds the same
/// object representation as a statically typed native-wrapper receiver.
fn emit_unboxed_receiver_metadata(ctx: &mut FunctionContext<'_>, object_reg: &str) {
    let object_reg = object_reg.to_string();
    abi::emit_store_to_sp(ctx.emitter, &object_reg, RECEIVER_OBJECT_OFFSET);
    let scratch = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let hidden = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    emit_wrapper_hidden_address(ctx, &object_reg, &hidden);
    abi::emit_load_from_address(ctx.emitter, &scratch, &hidden, 16);
    let handle = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_load_from_address(ctx.emitter, &handle, &hidden, 32);
    abi::emit_store_to_sp(ctx.emitter, &handle, 24);
    let context_ready = ctx.next_label("dom_receiver_context_ready");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbnz {}, {}", scratch, context_ready));  // retain the wrapper-owned native execution context
            abi::emit_call_label(ctx.emitter, "__rt_dom_context_ensure");
            ctx.emitter.instruction(&format!("mov {}, x0", scratch));           // lazily initialize a directly constructed stateless wrapper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", scratch, scratch)); // does this wrapper already retain a native execution context?
            ctx.emitter
                .instruction(&format!("jnz {}", context_ready));                // retain the wrapper-owned native execution context
            abi::emit_call_label(ctx.emitter, "__rt_dom_context_ensure");
            ctx.emitter.instruction(&format!("mov {}, rax", scratch));          // lazily initialize a directly constructed stateless wrapper
        }
    }
    ctx.emitter.label(&context_ready);
    abi::emit_store_to_sp(ctx.emitter, &scratch, 16);
}

/// Loads one direct or boxed wrapper value as an ordinary PHP object pointer.
fn emit_wrapper_object_pointer(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let boxed = wrapper_value_is_boxed(ctx, value)?;
    ctx.load_value_to_result(value)?;
    if !boxed {
        return Ok(());
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let object = ctx.next_label("dom_wrapper_union_object");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // require an object member from the boxed wrapper union
            ctx.emitter.instruction(&format!("b.eq {}", object));               // continue with the unboxed PHP object payload
            ctx.emitter.instruction("b __rt_dom_bridge_failure");               // reject null or scalar wrapper-union receivers
            ctx.emitter.label(&object);
            ctx.emitter.instruction("mov x0, x1");                              // promote the unboxed wrapper object pointer
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // require an object member from the boxed wrapper union
            ctx.emitter.instruction(&format!("je {}", object));                 // continue with the unboxed PHP object payload
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure");             // reject null or scalar wrapper-union receivers
            ctx.emitter.label(&object);
            ctx.emitter.instruction("mov rax, rdi");                            // promote the unboxed wrapper object pointer
        }
    }
    Ok(())
}

/// Validates one native-wrapper value and reports whether its runtime storage is boxed.
fn wrapper_value_is_boxed(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    match ctx.raw_value_php_type(value)? {
        PhpType::Object(class_name) => {
            let normalized = class_name.trim_start_matches('\\');
            if crate::internal_extensions::is_native_wrapper_class(normalized)
                || crate::internal_extensions::is_native_wrapper_descendant(
                    &ctx.module.class_infos,
                    normalized,
                )
            {
                Ok(false)
            } else {
                Err(CodegenIrError::unsupported(format!(
                    "non-native internal-extension receiver {normalized}"
                )))
            }
        }
        PhpType::Union(members) => {
            let mut found_wrapper = false;
            for member in members {
                match member {
                    PhpType::Object(class_name)
                        if crate::internal_extensions::is_native_wrapper_class(&class_name)
                            || crate::internal_extensions::is_native_wrapper_descendant(
                                &ctx.module.class_infos,
                                &class_name,
                            ) =>
                    {
                        found_wrapper = true;
                    }
                    PhpType::Void
                    | PhpType::False
                    | PhpType::Bool
                    | PhpType::Int
                    | PhpType::Float
                    | PhpType::Str => {}
                    _ => {
                        return Err(CodegenIrError::unsupported(
                            "dynamic internal-extension wrapper receiver",
                        ));
                    }
                }
            }
            if found_wrapper {
                Ok(true)
            } else {
                Err(CodegenIrError::unsupported(
                    "internal-extension union has no wrapper member",
                ))
            }
        }
        _ => Err(CodegenIrError::unsupported(
            "dynamic internal-extension wrapper receiver",
        )),
    }
}

/// Computes the flat request size, including nested values and every runtime byte payload.
fn emit_request_size(
    ctx: &mut FunctionContext<'_>,
    arguments: &[ValueId],
    opcode: u32,
    prepared_strings: &[PreparedStringArgument],
) -> Result<()> {
    let argument_count = i64::try_from(arguments.len())
        .map_err(|_| CodegenIrError::invalid_module("internal extension value count overflow"))?;
    store_stack_immediate(ctx, TOTAL_VALUE_COUNT_OFFSET, argument_count);
    store_stack_immediate(ctx, TOTAL_BYTE_COUNT_OFFSET, 0);
    for (index, argument) in arguments.iter().copied().enumerate() {
        if operation_parameter_is_flat_array(opcode, index) {
            emit_measure_runtime_value(
                ctx,
                argument,
                operation_parameter_prepares_xpath_callbacks(opcode, index),
            )?;
            continue;
        }
        let raw_type = ctx.raw_value_php_type(argument)?;
        if let Some(prepared) = prepared_string_argument(prepared_strings, index) {
            let (_, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                len_reg,
                prepared.stack_offset + 8,
            );
        } else if raw_type.codegen_repr() == PhpType::Str {
            ctx.load_value_to_result(argument)?;
        } else if is_string_backed_enum(ctx, &raw_type) {
            load_string_backed_enum_value(ctx, argument)?;
        } else {
            continue;
        }
        let (_, len_reg) = abi::string_result_regs(ctx.emitter);
        let total_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            &total_reg,
            TOTAL_BYTE_COUNT_OFFSET,
        );
        // -- accumulate root string bytes --
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter
                    .instruction(&format!("add {}, {}, {}", total_reg, total_reg, len_reg)); // include this string's exact byte length in the request byte section
            }
            Arch::X86_64 => {
                ctx.emitter
                    .instruction(&format!("add {}, {}", total_reg, len_reg));    // include this string's exact byte length in the request byte section
            }
        }
        abi::emit_store_to_sp(ctx.emitter, &total_reg, TOTAL_BYTE_COUNT_OFFSET);
    }
    emit_complete_request_size(ctx);
    Ok(())
}

/// Adds one array-valued argument's measured descendants and bytes to the request totals.
fn emit_measure_runtime_value(
    ctx: &mut FunctionContext<'_>,
    argument: ValueId,
    prepare_xpath_callbacks: bool,
) -> Result<()> {
    emit_stage_runtime_value(ctx, argument)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "x0",
                RUNTIME_VALUE_POINTER_OFFSET,
            );
            abi::emit_symbol_address(ctx.emitter, "x1", "_class_name_entries");
            abi::emit_load_symbol_to_reg(ctx.emitter, "x2", "_class_name_count", 0);
            abi::emit_load_int_immediate(ctx.emitter, "x3", RUNTIME_VALUE_MAX_DEPTH);
            abi::emit_temporary_stack_address(
                ctx.emitter,
                "x4",
                RUNTIME_VALUE_MEASURE_OFFSET,
            );
            if prepare_xpath_callbacks {
                abi::emit_temporary_stack_address(
                    ctx.emitter,
                    "x5",
                    PREPARED_XPATH_CALLBACK_VALUE_OFFSET,
                );
            }
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "rdi",
                RUNTIME_VALUE_POINTER_OFFSET,
            );
            abi::emit_symbol_address(ctx.emitter, "rsi", "_class_name_entries");
            abi::emit_load_symbol_to_reg(ctx.emitter, "rdx", "_class_name_count", 0);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", RUNTIME_VALUE_MAX_DEPTH);
            abi::emit_temporary_stack_address(
                ctx.emitter,
                "r8",
                RUNTIME_VALUE_MEASURE_OFFSET,
            );
            if prepare_xpath_callbacks {
                abi::emit_temporary_stack_address(
                    ctx.emitter,
                    "r9",
                    PREPARED_XPATH_CALLBACK_VALUE_OFFSET,
                );
            }
        }
    }
    let symbol = ctx.emitter.target.extern_symbol(if prepare_xpath_callbacks {
        "elephc_dom_prepare_xpath_callback_value"
    } else {
        "elephc_dom_measure_runtime_value"
    });
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_require_zero_status(ctx);
    let count = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let total = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &count,
        RUNTIME_VALUE_MEASURE_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, &total, TOTAL_VALUE_COUNT_OFFSET);
    // -- add serialized descendant records --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("sub {}, {}, #1", count, count));  // exclude the already reserved top-level argument record
            ctx.emitter.instruction(&format!("add {}, {}, {}", total, total, count)); // include every nested value record in the flat section
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("sub {}, 1", count));              // exclude the already reserved top-level argument record
            ctx.emitter.instruction(&format!("add {}, {}", total, count));      // include every nested value record in the flat section
        }
    }
    abi::emit_store_to_sp(ctx.emitter, &total, TOTAL_VALUE_COUNT_OFFSET);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &count,
        RUNTIME_VALUE_MEASURE_OFFSET + 8,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, &total, TOTAL_BYTE_COUNT_OFFSET);
    // -- add serialized descendant bytes --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("add {}, {}, {}", total, total, count)); // include nested keys, strings, and object class names
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("add {}, {}", total, count));      // include nested keys, strings, and object class names
        }
    }
    abi::emit_store_to_sp(ctx.emitter, &total, TOTAL_BYTE_COUNT_OFFSET);
    Ok(())
}

/// Converts one EIR array/null value into the borrowed three-word runtime-cell contract.
fn emit_stage_runtime_value(
    ctx: &mut FunctionContext<'_>,
    argument: ValueId,
) -> Result<()> {
    let raw_type = ctx.raw_value_php_type(argument)?;
    match raw_type.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(argument)?;
            abi::emit_store_to_sp(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                RUNTIME_VALUE_POINTER_OFFSET,
            );
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            ctx.load_value_to_result(argument)?;
            let runtime_tag = if matches!(raw_type.codegen_repr(), PhpType::Array(_)) {
                4
            } else {
                5
            };
            let element_tag_override = match raw_type.codegen_repr() {
                PhpType::Array(element)
                    if matches!(element.codegen_repr(), PhpType::Callable) =>
                {
                    10
                }
                _ => 0,
            };
            let payload = abi::int_result_reg(ctx.emitter).to_string();
            abi::emit_store_to_sp(ctx.emitter, &payload, RUNTIME_VALUE_OFFSET + 8);
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET, runtime_tag);
            store_stack_immediate(
                ctx,
                RUNTIME_VALUE_OFFSET + 16,
                element_tag_override,
            );
            let pointer = abi::secondary_scratch_reg(ctx.emitter).to_string();
            abi::emit_temporary_stack_address(ctx.emitter, &pointer, RUNTIME_VALUE_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, &pointer, RUNTIME_VALUE_POINTER_OFFSET);
        }
        PhpType::Str => {
            ctx.load_value_to_result(argument)?;
            let (pointer, length) = abi::string_result_regs(ctx.emitter);
            abi::emit_store_to_sp(ctx.emitter, pointer, RUNTIME_VALUE_OFFSET + 8);
            abi::emit_store_to_sp(ctx.emitter, length, RUNTIME_VALUE_OFFSET + 16);
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET, 1);
            let address = abi::secondary_scratch_reg(ctx.emitter).to_string();
            abi::emit_temporary_stack_address(ctx.emitter, &address, RUNTIME_VALUE_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, &address, RUNTIME_VALUE_POINTER_OFFSET);
        }
        PhpType::Int | PhpType::Bool | PhpType::False => {
            ctx.load_value_to_result(argument)?;
            abi::emit_store_to_sp(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                RUNTIME_VALUE_OFFSET + 8,
            );
            let runtime_tag = if matches!(raw_type.codegen_repr(), PhpType::Int) {
                0
            } else {
                3
            };
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET, runtime_tag);
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET + 16, 0);
            let address = abi::secondary_scratch_reg(ctx.emitter).to_string();
            abi::emit_temporary_stack_address(ctx.emitter, &address, RUNTIME_VALUE_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, &address, RUNTIME_VALUE_POINTER_OFFSET);
        }
        PhpType::Float => {
            ctx.load_value_to_result(argument)?;
            let payload = abi::secondary_scratch_reg(ctx.emitter).to_string();
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!(
                        "fmov {}, {}",
                        payload,
                        abi::float_result_reg(ctx.emitter)
                    ));                                                         // preserve the scalar's exact IEEE-754 payload bits
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!(
                        "movq {}, {}",
                        payload,
                        abi::float_result_reg(ctx.emitter)
                    ));                                                         // preserve the scalar's exact IEEE-754 payload bits
                }
            }
            abi::emit_store_to_sp(ctx.emitter, &payload, RUNTIME_VALUE_OFFSET + 8);
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET, 2);
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET + 16, 0);
            let address = abi::secondary_scratch_reg(ctx.emitter).to_string();
            abi::emit_temporary_stack_address(ctx.emitter, &address, RUNTIME_VALUE_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, &address, RUNTIME_VALUE_POINTER_OFFSET);
        }
        PhpType::Object(_) => {
            ctx.load_value_to_result(argument)?;
            abi::emit_store_to_sp(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                RUNTIME_VALUE_OFFSET + 8,
            );
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET, 6);
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET + 16, 0);
            let address = abi::secondary_scratch_reg(ctx.emitter).to_string();
            abi::emit_temporary_stack_address(ctx.emitter, &address, RUNTIME_VALUE_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, &address, RUNTIME_VALUE_POINTER_OFFSET);
        }
        PhpType::Callable => {
            ctx.load_value_to_result(argument)?;
            abi::emit_store_to_sp(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                RUNTIME_VALUE_OFFSET + 8,
            );
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET, 10);
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET + 16, 0);
            let address = abi::secondary_scratch_reg(ctx.emitter).to_string();
            abi::emit_temporary_stack_address(ctx.emitter, &address, RUNTIME_VALUE_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, &address, RUNTIME_VALUE_POINTER_OFFSET);
        }
        PhpType::Void => {
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET, 8);
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET + 8, 0);
            store_stack_immediate(ctx, RUNTIME_VALUE_OFFSET + 16, 0);
            let pointer = abi::secondary_scratch_reg(ctx.emitter).to_string();
            abi::emit_temporary_stack_address(ctx.emitter, &pointer, RUNTIME_VALUE_OFFSET);
            abi::emit_store_to_sp(ctx.emitter, &pointer, RUNTIME_VALUE_POINTER_OFFSET);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "flat internal-extension array argument type {other:?}"
            )));
        }
    }
    Ok(())
}

/// Derives and stores the final allocation size from dynamic value and byte counts.
fn emit_complete_request_size(ctx: &mut FunctionContext<'_>) {
    let values = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let total = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &values, TOTAL_VALUE_COUNT_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &total, TOTAL_BYTE_COUNT_OFFSET);
    // -- calculate the complete request allocation --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x8, #{}", ABI_VALUE_SIZE));   // materialize the fixed ABI value-record width
            ctx.emitter.instruction(&format!("madd {}, {}, x8, {}", total, values, total)); // add the complete flat value section to all bytes
            ctx.emitter.instruction(&format!("add {}, {}, #{}", total, total, REQUEST_HEADER_SIZE)); // include the padded request header
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("imul {}, {}", values, ABI_VALUE_SIZE)); // scale the flat value count to its byte width
            ctx.emitter.instruction(&format!("add {}, {}", total, values));     // add the complete flat value section to all bytes
            ctx.emitter.instruction(&format!("add {}, {}", total, REQUEST_HEADER_SIZE)); // include the padded request header
        }
    }
    abi::emit_store_to_sp(ctx.emitter, &total, 8);
}

/// Allocates the complete flat request from Elephc's checked runtime heap.
fn allocate_request(ctx: &mut FunctionContext<'_>) -> Result<()> {
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 8);
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0,
    );
    store_stack_immediate(ctx, 32, 0);
    Ok(())
}

/// Writes the request header and validates the compile-time count conversions.
fn emit_request_header(
    ctx: &mut FunctionContext<'_>,
    opcode: u32,
    value_count: usize,
) -> Result<()> {
    let value_count = i64::try_from(value_count)
        .map_err(|_| CodegenIrError::invalid_module("internal extension value count overflow"))?;
    if value_count >= REQUEST_FLAG_ARGUMENT_COUNT {
        return Err(CodegenIrError::invalid_module(
            "internal extension root argument count overflow",
        ));
    }
    let request_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let scratch = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &request_reg, 0);
    emit_store_u32_immediate(ctx, &request_reg, 0, ABI_VERSION);
    emit_store_u32_immediate(
        ctx,
        &request_reg,
        4,
        REQUEST_HEADER_SIZE as i64,
    );
    emit_store_u32_immediate(ctx, &request_reg, 8, i64::from(opcode));
    emit_store_u32_immediate(
        ctx,
        &request_reg,
        12,
        REQUEST_FLAG_ARGUMENT_COUNT | value_count,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, &scratch, 24);
    abi::emit_store_to_address(ctx.emitter, &scratch, &request_reg, 16);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &scratch, TOTAL_VALUE_COUNT_OFFSET);
    abi::emit_store_to_address(ctx.emitter, &scratch, &request_reg, 24);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &scratch, TOTAL_BYTE_COUNT_OFFSET);
    abi::emit_store_to_address(ctx.emitter, &scratch, &request_reg, 32);
    Ok(())
}

/// Initializes the caller-owned append context shared by all nested runtime values.
fn emit_runtime_value_write_context(
    ctx: &mut FunctionContext<'_>,
    argument_count: usize,
) -> Result<()> {
    let argument_count = i64::try_from(argument_count)
        .map_err(|_| CodegenIrError::invalid_module("internal extension value count overflow"))?;
    store_stack_immediate(ctx, RUNTIME_VALUE_MEASURE_OFFSET, argument_count);
    store_stack_immediate(ctx, 32, 0);
    let request = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let scratch = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &request, 0);
    // -- address the flat ABI value section --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("add {}, {}, #{}", scratch, request, REQUEST_HEADER_SIZE)); // address the complete flat ABI value section
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("lea {}, [{} + {}]", scratch, request, REQUEST_HEADER_SIZE)); // address the complete flat ABI value section
        }
    }
    abi::emit_store_to_sp(
        ctx.emitter,
        &scratch,
        RUNTIME_VALUE_WRITE_CONTEXT_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, &scratch, TOTAL_VALUE_COUNT_OFFSET);
    abi::emit_store_to_sp(
        ctx.emitter,
        &scratch,
        RUNTIME_VALUE_WRITE_CONTEXT_OFFSET + 8,
    );
    abi::emit_temporary_stack_address(ctx.emitter, &scratch, RUNTIME_VALUE_MEASURE_OFFSET);
    abi::emit_store_to_sp(
        ctx.emitter,
        &scratch,
        RUNTIME_VALUE_WRITE_CONTEXT_OFFSET + 16,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, &scratch, TOTAL_VALUE_COUNT_OFFSET);
    // -- address the request byte section --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x8, #{}", ABI_VALUE_SIZE));   // materialize one ABI value-record width
            ctx.emitter.instruction(&format!("madd {}, {}, x8, {}", scratch, scratch, request)); // advance from the request base by all value records
            ctx.emitter.instruction(&format!("add {}, {}, #{}", scratch, scratch, REQUEST_HEADER_SIZE)); // skip the padded request header
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("imul {}, {}", scratch, ABI_VALUE_SIZE)); // scale the dynamic flat value count
            ctx.emitter.instruction(&format!("add {}, {}", scratch, request));  // advance from the request base by all value records
            ctx.emitter.instruction(&format!("add {}, {}", scratch, REQUEST_HEADER_SIZE)); // skip the padded request header
        }
    }
    abi::emit_store_to_sp(
        ctx.emitter,
        &scratch,
        RUNTIME_VALUE_WRITE_CONTEXT_OFFSET + 24,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, &scratch, TOTAL_BYTE_COUNT_OFFSET);
    abi::emit_store_to_sp(
        ctx.emitter,
        &scratch,
        RUNTIME_VALUE_WRITE_CONTEXT_OFFSET + 32,
    );
    abi::emit_temporary_stack_address(ctx.emitter, &scratch, 32);
    abi::emit_store_to_sp(
        ctx.emitter,
        &scratch,
        RUNTIME_VALUE_WRITE_CONTEXT_OFFSET + 40,
    );
    abi::emit_symbol_address(ctx.emitter, &scratch, "_class_name_entries");
    abi::emit_store_to_sp(
        ctx.emitter,
        &scratch,
        RUNTIME_VALUE_WRITE_CONTEXT_OFFSET + 48,
    );
    abi::emit_load_symbol_to_reg(ctx.emitter, &scratch, "_class_name_count", 0);
    abi::emit_store_to_sp(
        ctx.emitter,
        &scratch,
        RUNTIME_VALUE_WRITE_CONTEXT_OFFSET + 56,
    );
    store_stack_immediate(
        ctx,
        RUNTIME_VALUE_WRITE_CONTEXT_OFFSET + 64,
        RUNTIME_VALUE_MAX_DEPTH,
    );
    Ok(())
}

/// Encodes one concrete EIR argument into its flat ABI value record.
fn emit_request_value(
    ctx: &mut FunctionContext<'_>,
    argument: ValueId,
    index: usize,
    opcode: u32,
    flags: u32,
    prepared_strings: &[PreparedStringArgument],
) -> Result<()> {
    let value_offset = REQUEST_HEADER_SIZE + index * ABI_VALUE_SIZE;
    if flags & FLAG_ARRAY_APPEND_OFFSET != 0
        && index == 0
        && matches!(opcode, SIMPLEXML_READ_DIMENSION_OPCODE | SIMPLEXML_WRITE_DIMENSION_OPCODE)
    {
        let request_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
        abi::emit_load_temporary_stack_slot(ctx.emitter, &request_reg, 0);
        emit_append_offset_value(ctx, &request_reg, value_offset);
        return Ok(());
    }
    let raw_type = ctx.raw_value_php_type(argument)?;
    if let Some(prepared) = prepared_string_argument(prepared_strings, index)
        .filter(|prepared| !prepared.runtime_polymorphic)
    {
        let (pointer, length) = abi::string_result_regs(ctx.emitter);
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            pointer,
            prepared.stack_offset,
        );
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            length,
            prepared.stack_offset + 8,
        );
        emit_string_value(ctx, value_offset)?;
        return Ok(());
    }
    if operation_parameter_is_flat_array(opcode, index) {
        emit_runtime_value(
            ctx,
            argument,
            value_offset,
            operation_parameter_prepares_xpath_callbacks(opcode, index),
        )?;
        return Ok(());
    }
    if operation_parameter_is_callable(opcode, index) {
        emit_callable_request_value(ctx, argument, &raw_type, value_offset)?;
        return Ok(());
    }
    if is_string_backed_enum(ctx, &raw_type) {
        load_string_backed_enum_value(ctx, argument)?;
        emit_string_value(ctx, value_offset)?;
        return Ok(());
    }
    if matches!(&raw_type, PhpType::Resource(_)) {
        ctx.load_value_to_result(argument)?;
        emit_host_handle_value(ctx, value_offset, 10);
        return Ok(());
    }
    if raw_type.codegen_repr() == PhpType::Mixed
        && operation_parameter_uses_dynamic_contract(opcode, index)
    {
        emit_mixed_native_wrapper_value(
            ctx,
            value_offset,
            opcode,
            index,
            prepared_strings,
        )?;
        return Ok(());
    }
    if guarded_simplexml_compare_wrapper_union(ctx, opcode, index, &raw_type) {
        emit_nullable_wrapper_value(ctx, argument, value_offset)?;
        return Ok(());
    }
    if nullable_wrapper_union(&raw_type) {
        emit_nullable_wrapper_value(ctx, argument, value_offset)?;
        return Ok(());
    }
    match raw_type.codegen_repr() {
        PhpType::Void => {
            let request_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
            abi::emit_load_temporary_stack_slot(ctx.emitter, &request_reg, 0);
            emit_zero_value(ctx, &request_reg, value_offset);
        }
        PhpType::Bool | PhpType::False => {
            ctx.load_value_to_result(argument)?;
            emit_scalar_value(ctx, value_offset, 1)?;
        }
        PhpType::Int => {
            ctx.load_value_to_result(argument)?;
            emit_scalar_value(ctx, value_offset, 2)?;
        }
        PhpType::Float => {
            ctx.load_value_to_result(argument)?;
            emit_float_value(ctx, value_offset)?;
        }
        PhpType::Str => {
            ctx.load_value_to_result(argument)?;
            emit_string_value(ctx, value_offset)?;
        }
        PhpType::Callable => {
            ctx.load_value_to_result(argument)?;
            emit_host_handle_value(ctx, value_offset, 9);
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            ctx.load_value_to_result(argument)?;
            emit_host_handle_value(ctx, value_offset, 7);
        }
        PhpType::Union(members) if nullable_callable_union(&members) => {
            emit_nullable_callable_value(ctx, argument, value_offset)?;
        }
        PhpType::Object(_) => {
            emit_object_value(ctx, argument, value_offset)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "internal extension argument type {other:?}"
            )));
        }
    }
    Ok(())
}

/// Recognizes the fallible SimpleXML comparison operand narrowed by an EIR runtime guard.
fn guarded_simplexml_compare_wrapper_union(
    ctx: &FunctionContext<'_>,
    opcode: u32,
    index: usize,
    php_type: &PhpType,
) -> bool {
    let Some(operation) = crate::internal_extensions::operation_registry().opcode(opcode) else {
        return false;
    };
    if index != 0
        || !operation.extension.eq_ignore_ascii_case("simplexml")
        || operation.kind != "object-handler"
        || operation.member != "compare"
    {
        return false;
    }
    let PhpType::Union(members) = php_type else {
        return false;
    };
    let mut wrapper = false;
    let mut failure = false;
    for member in members {
        match member {
            PhpType::Object(class_name)
                if super::mixed_internal_extension_class_is_a(
                    ctx,
                    class_name,
                    "SimpleXMLElement",
                ) =>
            {
                wrapper = true;
            }
            PhpType::False | PhpType::Void | PhpType::Never => failure = true,
            _ => return false,
        }
    }
    wrapper && failure
}

/// Reports whether one C14N parameter needs recursive value-tree marshalling.
fn operation_parameter_is_flat_array(opcode: u32, index: usize) -> bool {
    matches!(
        (opcode, index),
        (4234 | 4387, 2 | 3)
            | (4235 | 4388, 3 | 4)
            | (4285 | 4424, 0)
    )
}

/// Reports whether one flat parameter needs one-time nested XPath callable resolution.
fn operation_parameter_prepares_xpath_callbacks(opcode: u32, index: usize) -> bool {
    index == 0 && matches!(opcode, 4285 | 4424)
}

/// Serializes one staged array/null runtime value into its root and descendant records.
fn emit_runtime_value(
    ctx: &mut FunctionContext<'_>,
    argument: ValueId,
    value_offset: usize,
    prepared_xpath_callbacks: bool,
) -> Result<()> {
    if !prepared_xpath_callbacks {
        emit_stage_runtime_value(ctx, argument)?;
    }
    // -- serialize one root runtime value --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let source_offset = if prepared_xpath_callbacks {
                PREPARED_XPATH_CALLBACK_VALUE_OFFSET
            } else {
                RUNTIME_VALUE_POINTER_OFFSET
            };
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", source_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            ctx.emitter.instruction(&format!("add x1, x1, #{}", value_offset)); // address the caller-visible root value record
            abi::emit_temporary_stack_address(
                ctx.emitter,
                "x2",
                RUNTIME_VALUE_WRITE_CONTEXT_OFFSET,
            );
        }
        Arch::X86_64 => {
            let source_offset = if prepared_xpath_callbacks {
                PREPARED_XPATH_CALLBACK_VALUE_OFFSET
            } else {
                RUNTIME_VALUE_POINTER_OFFSET
            };
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", source_offset);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 0);
            ctx.emitter.instruction(&format!("add rsi, {}", value_offset));     // address the caller-visible root value record
            abi::emit_temporary_stack_address(
                ctx.emitter,
                "rdx",
                RUNTIME_VALUE_WRITE_CONTEXT_OFFSET,
            );
        }
    }
    let symbol = ctx.emitter.target.extern_symbol(if prepared_xpath_callbacks {
        "elephc_dom_write_prepared_xpath_callback_value"
    } else {
        "elephc_dom_write_runtime_value"
    });
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_require_zero_status(ctx);
    Ok(())
}

/// Fails closed when one compiler-to-bridge serialization helper rejects runtime storage.
fn emit_require_zero_status(ctx: &mut FunctionContext<'_>) {
    // -- fail closed on marshalling errors --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let success = ctx.next_label("dom_runtime_value_success");
            ctx.emitter.instruction("cmp w0, #0");                              // did the runtime-value bridge helper accept the borrowed storage?
            ctx.emitter.instruction(&format!("b.eq {}", success));              // continue after a complete checked serialization step
            ctx.emitter.instruction("b __rt_dom_bridge_failure");               // reject corrupt or oversized runtime container metadata
            ctx.emitter.label(&success);
        }
        Arch::X86_64 => {
            let success = ctx.next_label("dom_runtime_value_success");
            ctx.emitter.instruction("test eax, eax");                           // did the runtime-value bridge helper accept the borrowed storage?
            ctx.emitter.instruction(&format!("jz {}", success));                // continue after a complete checked serialization step
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure");             // reject corrupt or oversized runtime container metadata
            ctx.emitter.label(&success);
        }
    }
}

/// Reports whether one locked operation parameter has PHP's callable type.
fn operation_parameter_is_callable(opcode: u32, index: usize) -> bool {
    operation_parameter_php_type(opcode, index)
        .is_some_and(php_type_name_contains_callable)
}

/// Returns one reflected PHP parameter type for a generated bridge operation.
fn operation_parameter_php_type(opcode: u32, index: usize) -> Option<&'static str> {
    let operations = crate::internal_extensions::operation_registry();
    let Some(operation) = operations.opcode(opcode) else {
        return None;
    };
    let registry = crate::internal_extensions::registry();
    match operation.kind.as_str() {
        "function" => registry
            .function(&operation.member)
            .and_then(|function| {
                reflected_parameter_at(&function.signature.parameters, index)
            })
            .and_then(|parameter| parameter.php_type.as_deref()),
        "method" => operation
            .class
            .as_deref()
            .and_then(|class_name| registry.class(class_name))
            .and_then(|class| {
                class.methods.iter().find(|method| {
                    method
                        .signature
                        .name
                        .eq_ignore_ascii_case(&operation.member)
                })
            })
            .and_then(|method| {
                reflected_parameter_at(&method.signature.parameters, index)
            })
            .and_then(|parameter| parameter.php_type.as_deref()),
        "property-set" if index == 0 => operation
            .class
            .as_deref()
            .and_then(|class_name| registry.class(class_name))
            .and_then(|class| {
                class
                    .properties
                    .iter()
                    .find(|property| property.name == operation.member)
            })
            .and_then(|property| property.php_type.as_deref()),
        _ => None,
    }
}

/// Resolves a fixed parameter or the repeated tail parameter of a variadic signature.
fn reflected_parameter_at<'a>(
    parameters: &'a [crate::internal_extensions::ParameterSpec],
    index: usize,
) -> Option<&'a crate::internal_extensions::ParameterSpec> {
    parameters.get(index).or_else(|| {
        parameters
            .last()
            .filter(|parameter| parameter.variadic)
    })
}

/// Reports whether one Reflection type expression contains the callable atom.
fn php_type_name_contains_callable(php_type: &str) -> bool {
    php_type.split('|').any(|member| {
        member
            .trim()
            .trim_start_matches('?')
            .eq_ignore_ascii_case("callable")
    })
}

/// Reports whether one reflected parameter needs runtime wrapper/string validation.
fn operation_parameter_uses_dynamic_contract(opcode: u32, index: usize) -> bool {
    native_wrapper_parameter_contract(opcode, index).is_some()
}

/// Resolves one exact PHP wrapper, string, and nullable parameter contract.
fn native_wrapper_parameter_contract(
    opcode: u32,
    index: usize,
) -> Option<NativeWrapperParameterContract> {
    let operations = crate::internal_extensions::operation_registry();
    let operation = operations.opcode(opcode)?;
    let registry = crate::internal_extensions::registry();
    let (
        callable,
        property,
        position,
        name,
        expected_type,
        declared_allows_null,
        declared_allows_stringable,
        variadic,
    ) = match operation.kind.as_str() {
        "function" => {
            let signature = &registry.function(&operation.member)?.signature;
            let parameter = reflected_parameter_at(&signature.parameters, index)?;
            (
                operation.member.clone(),
                None,
                if parameter.variadic {
                    index + 1
                } else {
                    parameter.position + 1
                },
                (!parameter.variadic).then(|| parameter.name.clone()),
                parameter.php_type.clone()?,
                parameter.allows_null,
                true,
                parameter.variadic,
            )
        }
        "method" => {
            let class_name = operation.class.as_deref()?;
            let method = registry.class(class_name)?.methods.iter().find(|method| {
                method
                    .signature
                    .name
                    .eq_ignore_ascii_case(&operation.member)
            })?;
            let parameter = reflected_parameter_at(&method.signature.parameters, index)?;
            let reflected_type = parameter.php_type.clone();
            let expected_type = reflected_type.clone().or_else(|| {
                legacy_dom_variadic_node_or_string_type(
                    class_name,
                    &method.signature.name,
                    parameter,
                )
                .map(str::to_string)
            })?;
            (
                format!("{class_name}::{}", method.signature.name),
                None,
                if parameter.variadic {
                    index + 1
                } else {
                    parameter.position + 1
                },
                (!parameter.variadic).then(|| parameter.name.clone()),
                expected_type,
                reflected_type.is_some() && parameter.allows_null,
                !dom_variadic_node_or_string_rejects_stringable(
                    class_name,
                    &method.signature.name,
                    parameter,
                ),
                parameter.variadic,
            )
        }
        "property-set" if index == 0 => {
            let class_name = operation.class.as_deref()?;
            let property_spec = registry
                .class(class_name)?
                .properties
                .iter()
                .find(|property| property.name == operation.member)?;
            (
                String::new(),
                Some(format!("{class_name}::${}", operation.member)),
                0,
                None,
                property_spec.php_type.clone()?,
                false,
                true,
                false,
            )
        }
        _ => return None,
    };
    let mut allows_string = false;
    let mut allows_null = declared_allows_null || expected_type.trim_start().starts_with('?');
    let mut wrapper_bases = Vec::new();
    for member in expected_type.split('|') {
        let member = member.trim();
        let normalized = member.trim_start_matches('?').trim_start_matches('\\');
        if crate::internal_extensions::is_native_wrapper_class(normalized) {
            wrapper_bases.push(normalized.to_string());
        } else if normalized.eq_ignore_ascii_case("string") {
            allows_string = true;
        } else if normalized.eq_ignore_ascii_case("null") {
            allows_null = true;
            continue;
        } else {
            return None;
        }
    }
    if wrapper_bases.is_empty() && !allows_string {
        return None;
    }
    wrapper_bases.sort_by_key(|class_name| crate::names::php_symbol_key(class_name));
    wrapper_bases.dedup_by(|left, right| {
        crate::names::php_symbol_key(left) == crate::names::php_symbol_key(right)
    });
    Some(NativeWrapperParameterContract {
        callable,
        property,
        position,
        name,
        expected_type,
        allows_null,
        allows_string,
        allows_stringable: allows_string && declared_allows_stringable,
        variadic,
        wrapper_bases,
    })
}

/// Restores the legacy DOM node-or-string contract enforced manually by php-src.
fn legacy_dom_variadic_node_or_string_type(
    class_name: &str,
    method_name: &str,
    parameter: &crate::internal_extensions::ParameterSpec,
) -> Option<&'static str> {
    let legacy_class = matches!(
        class_name,
        "DOMCharacterData"
            | "DOMChildNode"
            | "DOMDocument"
            | "DOMDocumentFragment"
            | "DOMElement"
            | "DOMParentNode"
    );
    let tree_mutation_method = matches!(
        method_name.to_ascii_lowercase().as_str(),
        "after" | "append" | "before" | "prepend" | "replacechildren" | "replacewith"
    );
    (legacy_class && tree_mutation_method && parameter.variadic && parameter.php_type.is_none())
        .then_some("DOMNode|string")
}

/// Reports DOM's manual node-or-string parser, which rejects `Stringable` objects.
fn dom_variadic_node_or_string_rejects_stringable(
    class_name: &str,
    method_name: &str,
    parameter: &crate::internal_extensions::ParameterSpec,
) -> bool {
    let tree_mutation_owner = matches!(
        class_name,
        "DOMCharacterData"
            | "DOMChildNode"
            | "DOMDocument"
            | "DOMDocumentFragment"
            | "DOMElement"
            | "DOMParentNode"
            | "Dom\\CharacterData"
            | "Dom\\ChildNode"
            | "Dom\\Document"
            | "Dom\\DocumentFragment"
            | "Dom\\Element"
            | "Dom\\ParentNode"
    );
    let tree_mutation_method = matches!(
        method_name.to_ascii_lowercase().as_str(),
        "after" | "append" | "before" | "prepend" | "replacechildren" | "replacewith"
    );
    tree_mutation_owner && tree_mutation_method && parameter.variadic
}

/// Resolves runtime class IDs for every accepted native-wrapper base.
fn native_wrapper_base_class_ids(
    ctx: &FunctionContext<'_>,
    contract: &NativeWrapperParameterContract,
) -> Result<Vec<i64>> {
    let mut class_ids = Vec::with_capacity(contract.wrapper_bases.len());
    for base in &contract.wrapper_bases {
        let key = crate::names::php_symbol_key(base.trim_start_matches('\\'));
        let class_info = ctx
            .module
            .class_infos
            .iter()
            .find(|(class_name, _)| {
                crate::names::php_symbol_key(class_name.trim_start_matches('\\')) == key
            })
            .map(|(_, class_info)| class_info)
            .ok_or_else(|| {
                CodegenIrError::invalid_module(format!(
                    "native-wrapper base {base} has no runtime class metadata"
                ))
            })?;
        class_ids.push(class_info.class_id as i64);
    }
    class_ids.sort_unstable();
    class_ids.dedup();
    Ok(class_ids)
}

/// Validates one boxed dynamic wrapper argument without allocating a bridge request.
fn emit_mixed_native_wrapper_contract_validation(
    ctx: &mut FunctionContext<'_>,
    argument: ValueId,
    contract: &NativeWrapperParameterContract,
    prepared_string: Option<PreparedStringArgument>,
) -> Result<()> {
    let class_ids = native_wrapper_base_class_ids(ctx, contract)?;
    let prepared = prepared_string.ok_or_else(|| {
        CodegenIrError::invalid_module(
            "dynamic internal-extension argument has no preflight snapshot",
        )
    })?;
    let result = abi::int_result_reg(ctx.emitter).to_string();
    emit_load_prepared_mixed_value(ctx, prepared, TRY_HANDLER_SLOT_SIZE, &result);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let object = ctx.next_label("dom_preflight_wrapper_object");
    let string = ctx.next_label("dom_preflight_wrapper_string");
    let null = ctx.next_label("dom_preflight_wrapper_null");
    let integer = ctx.next_label("dom_preflight_wrapper_int");
    let float = ctx.next_label("dom_preflight_wrapper_float");
    let boolean = ctx.next_label("dom_preflight_wrapper_bool");
    let false_boolean = ctx.next_label("dom_preflight_wrapper_false");
    let array = ctx.next_label("dom_preflight_wrapper_array");
    let resource = ctx.next_label("dom_preflight_wrapper_resource");
    let closure = ctx.next_label("dom_preflight_wrapper_closure");
    let done = ctx.next_label("dom_preflight_wrapper_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // is the dynamic value an object?
            ctx.emitter.instruction(&format!("b.eq {}", object));               // validate native-wrapper inheritance
            ctx.emitter.instruction("cmp x0, #1");                              // is the dynamic value a string?
            ctx.emitter.instruction(&format!("b.eq {}", string));               // accept or reject the string union member
            ctx.emitter.instruction("cmp x0, #8");                              // is the dynamic value null?
            ctx.emitter.instruction(&format!("b.eq {}", null));                 // accept or reject the nullable member
            ctx.emitter.instruction("cmp x0, #0");                              // is the dynamic value an integer?
            ctx.emitter.instruction(&format!("b.eq {}", integer));              // report PHP's integer type name
            ctx.emitter.instruction("cmp x0, #2");                              // is the dynamic value a float?
            ctx.emitter.instruction(&format!("b.eq {}", float));                // report PHP's float type name
            ctx.emitter.instruction("cmp x0, #3");                              // is the dynamic value a boolean?
            ctx.emitter.instruction(&format!("b.eq {}", boolean));              // report PHP's boolean type name
            ctx.emitter.instruction("cmp x0, #4");                              // is the dynamic value a list?
            ctx.emitter.instruction(&format!("b.eq {}", array));                // both array shapes report array
            ctx.emitter.instruction("cmp x0, #5");                              // is the dynamic value an associative array?
            ctx.emitter.instruction(&format!("b.eq {}", array));                // both array shapes report array
            ctx.emitter.instruction("cmp x0, #9");                              // is the dynamic value a resource?
            ctx.emitter.instruction(&format!("b.eq {}", resource));             // report PHP's resource type name
            ctx.emitter.instruction("cmp x0, #10");                             // is the dynamic value a Closure?
            ctx.emitter.instruction(&format!("b.eq {}", closure));              // report PHP's Closure class name
            ctx.emitter.instruction("b __rt_dom_bridge_failure");               // reject corrupt Mixed runtime tags
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // is the dynamic value an object?
            ctx.emitter.instruction(&format!("je {}", object));                 // validate native-wrapper inheritance
            ctx.emitter.instruction("cmp rax, 1");                              // is the dynamic value a string?
            ctx.emitter.instruction(&format!("je {}", string));                 // accept or reject the string union member
            ctx.emitter.instruction("cmp rax, 8");                              // is the dynamic value null?
            ctx.emitter.instruction(&format!("je {}", null));                   // accept or reject the nullable member
            ctx.emitter.instruction("cmp rax, 0");                              // is the dynamic value an integer?
            ctx.emitter.instruction(&format!("je {}", integer));                // report PHP's integer type name
            ctx.emitter.instruction("cmp rax, 2");                              // is the dynamic value a float?
            ctx.emitter.instruction(&format!("je {}", float));                  // report PHP's float type name
            ctx.emitter.instruction("cmp rax, 3");                              // is the dynamic value a boolean?
            ctx.emitter.instruction(&format!("je {}", boolean));                // report PHP's boolean type name
            ctx.emitter.instruction("cmp rax, 4");                              // is the dynamic value a list?
            ctx.emitter.instruction(&format!("je {}", array));                  // both array shapes report array
            ctx.emitter.instruction("cmp rax, 5");                              // is the dynamic value an associative array?
            ctx.emitter.instruction(&format!("je {}", array));                  // both array shapes report array
            ctx.emitter.instruction("cmp rax, 9");                              // is the dynamic value a resource?
            ctx.emitter.instruction(&format!("je {}", resource));               // report PHP's resource type name
            ctx.emitter.instruction("cmp rax, 10");                             // is the dynamic value a Closure?
            ctx.emitter.instruction(&format!("je {}", closure));                // report PHP's Closure class name
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure");             // reject corrupt Mixed runtime tags
        }
    }

    ctx.emitter.label(&object);
    for class_id in class_ids {
        let first_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
        let target_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
        let kind_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
        emit_load_prepared_mixed_value(
            ctx,
            prepared,
            TRY_HANDLER_SLOT_SIZE,
            first_arg,
        );
        abi::emit_load_int_immediate(ctx.emitter, target_arg, class_id);
        abi::emit_load_int_immediate(ctx.emitter, kind_arg, 0);
        abi::emit_call_label(ctx.emitter, "__rt_mixed_instanceof");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cbnz x0, {}", done));         // accept this wrapper base or any descendant
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test rax, rax");                       // did this wrapper base match the object?
                ctx.emitter.instruction(&format!("jne {}", done));              // accept this wrapper base or any descendant
            }
        }
    }
    if contract.allows_stringable {
        emit_mixed_stringable_object_preparation(
            ctx,
            argument,
            contract,
            prepared,
            &done,
        )?;
    } else {
        emit_native_wrapper_object_type_error(ctx, prepared, contract)?;
    }

    ctx.emitter.label(&string);
    if contract.allows_string {
        emit_stage_owned_string_result(
            ctx,
            prepared_preflight_stack_offset(prepared),
        );
        abi::emit_jump(ctx.emitter, &done);
    } else {
        emit_native_wrapper_type_error(ctx, contract, "string");
    }

    ctx.emitter.label(&null);
    if contract.allows_null {
        abi::emit_jump(ctx.emitter, &done);
    } else {
        emit_native_wrapper_type_error(ctx, contract, "null");
    }

    ctx.emitter.label(&integer);
    emit_native_wrapper_type_error(ctx, contract, "int");
    ctx.emitter.label(&float);
    emit_native_wrapper_type_error(ctx, contract, "float");
    ctx.emitter.label(&boolean);
    if contract.variadic {
        emit_native_wrapper_type_error(ctx, contract, "bool");
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x1, #0");                          // distinguish false from true for fixed parameters
                ctx.emitter.instruction(&format!("b.eq {}", false_boolean));    // report PHP's literal false type name
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp rdi, 0");                          // distinguish false from true for fixed parameters
                ctx.emitter.instruction(&format!("je {}", false_boolean));      // report PHP's literal false type name
            }
        }
        emit_native_wrapper_type_error(ctx, contract, "true");
        ctx.emitter.label(&false_boolean);
        emit_native_wrapper_type_error(ctx, contract, "false");
    }
    ctx.emitter.label(&array);
    emit_native_wrapper_type_error(ctx, contract, "array");
    ctx.emitter.label(&resource);
    emit_native_wrapper_type_error(ctx, contract, "resource");
    ctx.emitter.label(&closure);
    emit_native_wrapper_type_error(ctx, contract, "Closure");
    ctx.emitter.label(&done);
    Ok(())
}

/// Dispatches one boxed object through its concrete `__toString()` implementation.
fn emit_mixed_stringable_object_preparation(
    ctx: &mut FunctionContext<'_>,
    argument: ValueId,
    contract: &NativeWrapperParameterContract,
    prepared: PreparedStringArgument,
    done_label: &str,
) -> Result<()> {
    let candidates = super::mixed_method_candidates(ctx, "__toString", 1)?;
    let receiver_reg = abi::nested_call_reg(ctx.emitter);
    let no_match = ctx.next_label("dom_preflight_stringable_no_match");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "dom_preflight_stringable_{}",
                super::label_fragment(&candidate.class_name)
            ))
        })
        .collect::<Vec<_>>();

    let result = abi::int_result_reg(ctx.emitter).to_string();
    emit_load_prepared_mixed_value(
        ctx,
        prepared,
        TRY_HANDLER_SLOT_SIZE,
        &result,
    );
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // require the prevalidated object tag after earlier coercions
            ctx.emitter.instruction(&format!("b.ne {}", no_match));             // reject storage that ceased to contain an object
            ctx.emitter
                .instruction(&format!("mov {}, x1", receiver_reg));             // preserve the concrete Stringable receiver across dispatch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // require the prevalidated object tag after earlier coercions
            ctx.emitter.instruction(&format!("jne {}", no_match));              // reject storage that ceased to contain an object
            ctx.emitter
                .instruction(&format!("mov {}, rdi", receiver_reg));            // preserve the concrete Stringable receiver across dispatch
        }
    }
    super::emit_mixed_method_class_dispatch(
        ctx,
        receiver_reg,
        &candidates,
        &match_labels,
        &no_match,
    );
    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let return_type = super::conversions::emit_mixed_tostring_candidate_call(
            ctx,
            argument,
            receiver_reg,
            candidate,
        )?;
        super::conversions::coerce_tostring_return_to_string_result(
            ctx,
            &return_type,
        )?;
        emit_stage_stringable_result(
            ctx,
            prepared_preflight_stack_offset(prepared),
        );
        abi::emit_jump(ctx.emitter, done_label);
    }
    ctx.emitter.label(&no_match);
    emit_native_wrapper_object_type_error(ctx, prepared, contract)
}

/// Throws a dynamic TypeError whose given type is the rejected object's class name.
fn emit_native_wrapper_object_type_error(
    ctx: &mut FunctionContext<'_>,
    prepared: PreparedStringArgument,
    contract: &NativeWrapperParameterContract,
) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter).to_string();
    emit_load_prepared_mixed_value(
        ctx,
        prepared,
        TRY_HANDLER_SLOT_SIZE,
        &result,
    );
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let unknown = ctx.next_label("dom_wrapper_type_error_unknown_object");
    let (prefix, suffix) = if let Some(property) = &contract.property {
        (
            "Cannot assign ".to_string(),
            format!(" to property {property} of type {}", contract.expected_type),
        )
    } else {
        let parameter_name = contract
            .name
            .as_ref()
            .map(|name| format!(" (${name})"))
            .unwrap_or_default();
        (
            format!(
                "{}(): Argument #{}{} must be of type {}, ",
                contract.callable,
                contract.position,
                parameter_name,
                contract.expected_type,
            ),
            " given".to_string(),
        )
    };
    let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
    let (suffix_label, suffix_len) = ctx.data.add_string(suffix.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // require the rejected value to remain an object
            ctx.emitter.instruction(&format!("b.ne {}", unknown));              // fall back safely for corrupt dynamic storage
            ctx.emitter.instruction("ldr x9, [x1]");                            // load the rejected object's runtime class id
            abi::emit_symbol_address(ctx.emitter, "x10", "_class_name_count");
            ctx.emitter.instruction("ldr x10, [x10]");                          // load the number of dense class-name lookup rows
            ctx.emitter.instruction("cmp x9, x10");                             // validate the class-name table index
            ctx.emitter.instruction(&format!("b.hs {}", unknown));              // unknown class ids report object
            abi::emit_symbol_address(ctx.emitter, "x11", "_class_name_entries");
            ctx.emitter.instruction("lsl x12, x9, #4");                         // scale the class id by one metadata row
            ctx.emitter.instruction("add x11, x11, x12");                       // address the concrete class-name row
            ctx.emitter.instruction("ldr x3, [x11]");                           // load the concrete class-name pointer
            ctx.emitter.instruction("ldr x4, [x11, #8]");                       // load the concrete class-name byte length
            abi::emit_symbol_address(ctx.emitter, "x1", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", prefix_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
            abi::emit_symbol_address(ctx.emitter, "x3", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", suffix_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // require the rejected value to remain an object
            ctx.emitter.instruction(&format!("jne {}", unknown));               // fall back safely for corrupt dynamic storage
            ctx.emitter.instruction("mov r8, QWORD PTR [rdi]");                 // load the rejected object's runtime class id
            abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_class_name_count", 0);
            ctx.emitter.instruction("cmp r8, r9");                              // validate the class-name table index
            ctx.emitter.instruction(&format!("jae {}", unknown));               // unknown class ids report object
            abi::emit_symbol_address(ctx.emitter, "r10", "_class_name_entries");
            ctx.emitter.instruction("shl r8, 4");                               // scale the class id by one metadata row
            ctx.emitter.instruction("add r10, r8");                             // address the concrete class-name row
            ctx.emitter.instruction("mov rdi, QWORD PTR [r10]");                // load the concrete class-name pointer
            ctx.emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");            // load the concrete class-name byte length
            abi::emit_symbol_address(ctx.emitter, "rax", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", prefix_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
            abi::emit_symbol_address(ctx.emitter, "rdi", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", suffix_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_concat");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    super::exceptions::emit_type_error_from_current_string(ctx);
    ctx.emitter.label(&unknown);
    emit_native_wrapper_type_error(ctx, contract, "object");
    Ok(())
}

/// Throws PHP's exact catchable `TypeError` for a rejected runtime argument.
fn emit_native_wrapper_type_error(
    ctx: &mut FunctionContext<'_>,
    contract: &NativeWrapperParameterContract,
    given_type: &str,
) {
    let message = if let Some(property) = &contract.property {
        format!(
            "Cannot assign {} to property {} of type {}",
            given_type, property, contract.expected_type,
        )
    } else {
        let parameter_name = contract
            .name
            .as_ref()
            .map(|name| format!(" (${name})"))
            .unwrap_or_default();
        format!(
            "{}(): Argument #{}{} must be of type {}, {} given",
            contract.callable,
            contract.position,
            parameter_name,
            contract.expected_type,
            given_type,
        )
    };
    super::exceptions::emit_type_error(ctx, &message);
}

/// Encodes any PHP callable representation as one borrowed descriptor handle.
fn emit_callable_request_value(
    ctx: &mut FunctionContext<'_>,
    argument: ValueId,
    raw_type: &PhpType,
    value_offset: usize,
) -> Result<()> {
    let owned_descriptor_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    match raw_type.codegen_repr() {
        PhpType::Void => {
            let request_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
            abi::emit_load_temporary_stack_slot(ctx.emitter, &request_reg, 0);
            emit_zero_value(ctx, &request_reg, value_offset);
            return Ok(());
        }
        PhpType::Callable => {
            ctx.load_value_to_result(argument)?;
            abi::emit_load_int_immediate(ctx.emitter, &owned_descriptor_reg, 0);
        }
        PhpType::Str => {
            let result_reg = abi::int_result_reg(ctx.emitter).to_string();
            super::callables::emit_runtime_string_descriptor_value(
                ctx,
                argument,
                &result_reg,
                "internal_extension_callable",
                false,
            )?;
            abi::emit_load_int_immediate(ctx.emitter, &owned_descriptor_reg, 0);
        }
        PhpType::Array(elem)
            if matches!(elem.codegen_repr(), PhpType::Mixed | PhpType::Str) =>
        {
            super::callables::emit_runtime_callable_array_descriptor_value_with_ownership(
                ctx,
                argument,
                &owned_descriptor_reg,
                "internal_extension_callable",
            )?;
        }
        PhpType::AssocArray { .. } => {
            super::callables::emit_runtime_assoc_callable_array_descriptor_value_with_ownership(
                ctx,
                argument,
                &owned_descriptor_reg,
                "internal_extension_callable",
            )?;
        }
        PhpType::Object(class_name) => {
            super::callables::emit_invokable_object_descriptor_value(
                ctx,
                argument,
                &class_name,
                "internal_extension_callable",
            )?;
            ctx.emitter.instruction(&format!(
                "mov {}, {}",
                owned_descriptor_reg,
                abi::int_result_reg(ctx.emitter)
            )); // mark the receiver-bound descriptor for post-call release
        }
        PhpType::Mixed | PhpType::Union(_) => {
            super::callables::emit_boxed_callable_descriptor_value(
                ctx,
                argument,
                &owned_descriptor_reg,
                "internal_extension_callable",
            )?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "internal extension callable argument type {other:?}"
            )));
        }
    }
    abi::emit_store_to_sp(
        ctx.emitter,
        &owned_descriptor_reg,
        TEMP_CALLABLE_DESCRIPTOR_OFFSET,
    );
    emit_host_handle_value(ctx, value_offset, 9);
    Ok(())
}

/// Reports whether one ordinary object type is a string-backed enum singleton.
fn is_string_backed_enum(
    ctx: &FunctionContext<'_>,
    php_type: &PhpType,
) -> bool {
    let PhpType::Object(class_name) = php_type else {
        return false;
    };
    let normalized = class_name.trim_start_matches('\\');
    ctx.module
        .enum_infos
        .get(normalized)
        .and_then(|info| info.backing_type.as_ref())
        .is_some_and(|backing| backing.codegen_repr() == PhpType::Str)
}

/// Loads a string-backed enum singleton's backing pointer and length as a string result.
fn load_string_backed_enum_value(
    ctx: &mut FunctionContext<'_>,
    argument: ValueId,
) -> Result<()> {
    ctx.load_value_to_result(argument)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x2, [x0, #16]");                       // load the enum singleton's backing-string length
            ctx.emitter.instruction("ldr x1, [x0, #8]");                        // load the enum singleton's backing-string pointer
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // preserve the enum singleton object pointer
            ctx.emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");           // load the enum singleton's backing-string length
            ctx.emitter.instruction("mov rax, QWORD PTR [r10 + 8]");            // load the enum singleton's backing-string pointer
        }
    }
    Ok(())
}

/// Returns true when a boxed union contains only native DOM wrappers and PHP null.
fn nullable_wrapper_union(php_type: &PhpType) -> bool {
    let PhpType::Union(members) = php_type else {
        return false;
    };
    let mut wrapper = false;
    let mut null = false;
    for member in members {
        match member.codegen_repr() {
            PhpType::Object(class_name)
                if crate::internal_extensions::is_native_wrapper_class(
                    &class_name,
                ) =>
            {
                wrapper = true;
            }
            PhpType::Void => null = true,
            _ => return false,
        }
    }
    wrapper && null
}

/// Returns true when a boxed union can only contain a callable descriptor or PHP null.
fn nullable_callable_union(members: &[PhpType]) -> bool {
    members.len() == 2
        && members.iter().any(|member| member.codegen_repr() == PhpType::Callable)
        && members.iter().any(|member| member.codegen_repr() == PhpType::Void)
}

/// Writes one borrowed host value, callable, or resource handle from the result register.
fn emit_host_handle_value(ctx: &mut FunctionContext<'_>, value_offset: usize, tag: i64) {
    let payload_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_store_to_sp(ctx.emitter, &payload_reg, TEMP_RESULT_LO_OFFSET);
    let request_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &request_reg, 0);
    emit_store_u32_immediate(ctx, &request_reg, value_offset, tag);
    emit_store_u32_immediate(ctx, &request_reg, value_offset + 4, 0);
    let payload_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &payload_reg, TEMP_RESULT_LO_OFFSET);
    abi::emit_store_to_address(
        ctx.emitter,
        &payload_reg,
        &request_reg,
        value_offset + 8,
    );
    abi::emit_store_zero_to_address(ctx.emitter, &request_reg, value_offset + 16);
}

/// Unboxes one nullable-callable union into a callable or null flat ABI value.
fn emit_nullable_callable_value(
    ctx: &mut FunctionContext<'_>,
    argument: ValueId,
    value_offset: usize,
) -> Result<()> {
    ctx.load_value_to_result(argument)?;
    let callable = ctx.next_label("dom_request_nullable_callable");
    let null = ctx.next_label("dom_request_nullable_callable_null");
    let done = ctx.next_label("dom_request_nullable_callable_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the boxed nullable-callable runtime tag
            ctx.emitter.instruction("cmp x9, #8");                              // is the union member PHP null?
            ctx.emitter
                .instruction(&format!("b.eq {}", null));                        // encode a null ABI record
            ctx.emitter.instruction(&format!("b {}", callable));                // materialize every non-null PHP callable shape
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rax], 8");                  // is the union member PHP null?
            ctx.emitter
                .instruction(&format!("je {}", null));                          // encode a null ABI record
            ctx.emitter.instruction(&format!("jmp {}", callable));              // materialize every non-null PHP callable shape
        }
    }
    ctx.emitter.label(&callable);
    let owned_descriptor_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    super::callables::emit_boxed_callable_descriptor_value(
        ctx,
        argument,
        &owned_descriptor_reg,
        "internal_extension_callable",
    )?;
    abi::emit_store_to_sp(
        ctx.emitter,
        &owned_descriptor_reg,
        TEMP_CALLABLE_DESCRIPTOR_OFFSET,
    );
    emit_host_handle_value(ctx, value_offset, 9);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&null);
    let request_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &request_reg, 0);
    emit_zero_value(ctx, &request_reg, value_offset);
    ctx.emitter.label(&done);
    Ok(())
}

/// Unboxes one nullable DOM-wrapper union into a bridge handle or null ABI value.
fn emit_nullable_wrapper_value(
    ctx: &mut FunctionContext<'_>,
    argument: ValueId,
    value_offset: usize,
) -> Result<()> {
    ctx.load_value_to_result(argument)?;
    let wrapper = ctx.next_label("dom_request_nullable_wrapper");
    let null = ctx.next_label("dom_request_nullable_wrapper_null");
    let done = ctx.next_label("dom_request_nullable_wrapper_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the boxed nullable-wrapper runtime tag
            ctx.emitter.instruction("cmp x9, #6");                              // is the union member a PHP object?
            ctx.emitter
                .instruction(&format!("b.eq {}", wrapper));                     // encode the native wrapper payload
            ctx.emitter.instruction("cmp x9, #8");                              // is the union member PHP null?
            ctx.emitter
                .instruction(&format!("b.eq {}", null));                        // encode a null ABI record
            ctx.emitter.instruction("b __rt_dom_bridge_failure");               // reject a corrupt nullable-wrapper union
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // load the concrete DOM wrapper object
            emit_object_pointer_value(ctx, value_offset)?;
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the nullable branch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rax], 6");                  // is the union member a PHP object?
            ctx.emitter
                .instruction(&format!("je {}", wrapper));                       // encode the native wrapper payload
            ctx.emitter.instruction("cmp QWORD PTR [rax], 8");                  // is the union member PHP null?
            ctx.emitter
                .instruction(&format!("je {}", null));                          // encode a null ABI record
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure");             // reject a corrupt nullable-wrapper union
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");            // load the concrete DOM wrapper object
            emit_object_pointer_value(ctx, value_offset)?;
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the nullable branch
        }
    }
    ctx.emitter.label(&null);
    let request_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &request_reg, 0);
    emit_zero_value(ctx, &request_reg, value_offset);
    ctx.emitter.label(&done);
    Ok(())
}

/// Encodes one prevalidated `Mixed` wrapper, string, or null bridge value.
fn emit_mixed_native_wrapper_value(
    ctx: &mut FunctionContext<'_>,
    value_offset: usize,
    opcode: u32,
    index: usize,
    prepared_strings: &[PreparedStringArgument],
) -> Result<()> {
    let contract = native_wrapper_parameter_contract(opcode, index).ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "native-wrapper operation {opcode} argument {index} has no callable contract"
        ))
    })?;
    let prepared_dynamic = prepared_string_argument(prepared_strings, index)
        .filter(|prepared| prepared.runtime_polymorphic);
    let prepared_dynamic = prepared_dynamic.ok_or_else(|| {
        CodegenIrError::invalid_module(
            "dynamic native-wrapper argument has no snapshotted Mixed value",
        )
    })?;
    let mixed_string = contract.allows_string.then_some(prepared_dynamic);
    let wrapper = ctx.next_label("dom_request_mixed_wrapper");
    let string = ctx.next_label("dom_request_mixed_wrapper_string");
    let null = ctx.next_label("dom_request_mixed_wrapper_null");
    let done = ctx.next_label("dom_request_mixed_wrapper_done");
    if let Some(prepared) = mixed_string {
        let tag = abi::secondary_scratch_reg(ctx.emitter).to_string();
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            &tag,
            prepared.stack_offset + PREPARED_STRING_TAG_OFFSET,
        );
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cbnz {}, {}", tag, string));  // encode a Stringable or scalar string staged during preflight
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("test {}, {}", tag, tag));     // was a string selected during dynamic preflight?
                ctx.emitter.instruction(&format!("jnz {}", string));            // encode a Stringable or scalar string staged during preflight
            }
        }
    }
    let result = abi::int_result_reg(ctx.emitter).to_string();
    emit_load_prepared_mixed_value(
        ctx,
        prepared_dynamic,
        CALL_FRAME_SIZE,
        &result,
    );
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // is the unpacked value a PHP object?
            ctx.emitter
                .instruction(&format!("b.eq {}", wrapper));                     // encode the native wrapper payload
            ctx.emitter.instruction("cmp x0, #8");                              // is the unpacked value PHP null?
            ctx.emitter
                .instruction(&format!("b.eq {}", null));                        // encode a null ABI record
            ctx.emitter.instruction("b __rt_dom_bridge_failure");               // reject post-validation storage corruption
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov x0, x1");                              // promote the unpacked wrapper object pointer
            emit_object_pointer_value(ctx, value_offset)?;
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the nullable branch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // is the unpacked value a PHP object?
            ctx.emitter
                .instruction(&format!("je {}", wrapper));                       // encode the native wrapper payload
            ctx.emitter.instruction("cmp rax, 8");                              // is the unpacked value PHP null?
            ctx.emitter
                .instruction(&format!("je {}", null));                          // encode a null ABI record
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure");             // reject post-validation storage corruption
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rax, rdi");                            // promote the unpacked wrapper object pointer
            emit_object_pointer_value(ctx, value_offset)?;
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the nullable branch
        }
    }
    if let Some(prepared) = mixed_string {
        ctx.emitter.label(&string);
        let (pointer, length) = abi::string_result_regs(ctx.emitter);
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            pointer,
            prepared.stack_offset,
        );
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            length,
            prepared.stack_offset + 8,
        );
        emit_string_value(ctx, value_offset)?;
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&null);
    if contract.allows_null {
        let request_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
        abi::emit_load_temporary_stack_slot(ctx.emitter, &request_reg, 0);
        emit_zero_value(ctx, &request_reg, value_offset);
    } else {
        abi::emit_jump(ctx.emitter, "__rt_dom_bridge_failure");
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Writes a null ABI value record.
fn emit_zero_value(ctx: &mut FunctionContext<'_>, request_reg: &str, value_offset: usize) {
    emit_store_u32_immediate(ctx, request_reg, value_offset, 0);
    emit_store_u32_immediate(ctx, request_reg, value_offset + 4, 0);
    abi::emit_store_zero_to_address(ctx.emitter, request_reg, value_offset + 8);
    abi::emit_store_zero_to_address(ctx.emitter, request_reg, value_offset + 16);
}

/// Writes the ABI-only SimpleXML append-offset marker for an empty `[]` dimension.
fn emit_append_offset_value(
    ctx: &mut FunctionContext<'_>,
    request_reg: &str,
    value_offset: usize,
) {
    emit_store_u32_immediate(ctx, request_reg, value_offset, 12);
    emit_store_u32_immediate(ctx, request_reg, value_offset + 4, 0);
    abi::emit_store_zero_to_address(ctx.emitter, request_reg, value_offset + 8);
    abi::emit_store_zero_to_address(ctx.emitter, request_reg, value_offset + 16);
}

/// Writes one boolean or integer ABI scalar from the current result register.
fn emit_scalar_value(
    ctx: &mut FunctionContext<'_>,
    value_offset: usize,
    tag: i64,
) -> Result<()> {
    let payload_reg = abi::int_result_reg(ctx.emitter).to_string();
    let request_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &request_reg, 0);
    emit_store_u32_immediate(ctx, &request_reg, value_offset, tag);
    emit_store_u32_immediate(ctx, &request_reg, value_offset + 4, 0);
    abi::emit_store_to_address(
        ctx.emitter,
        &payload_reg,
        &request_reg,
        value_offset + 8,
    );
    abi::emit_store_zero_to_address(ctx.emitter, &request_reg, value_offset + 16);
    Ok(())
}

/// Writes one IEEE-754 payload into a floating-point ABI value record.
fn emit_float_value(ctx: &mut FunctionContext<'_>, value_offset: usize) -> Result<()> {
    let request_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let payload_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("fmov {}, {}", payload_reg, abi::float_result_reg(ctx.emitter))); // preserve the exact IEEE-754 payload bits
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("movq {}, {}", payload_reg, abi::float_result_reg(ctx.emitter))); // preserve the exact IEEE-754 payload bits
        }
    }
    abi::emit_store_to_sp(ctx.emitter, &payload_reg, TEMP_RESULT_LO_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &request_reg, 0);
    emit_store_u32_immediate(ctx, &request_reg, value_offset, 3);
    emit_store_u32_immediate(ctx, &request_reg, value_offset + 4, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &payload_reg, TEMP_RESULT_LO_OFFSET);
    abi::emit_store_to_address(
        ctx.emitter,
        &payload_reg,
        &request_reg,
        value_offset + 8,
    );
    abi::emit_store_zero_to_address(ctx.emitter, &request_reg, value_offset + 16);
    Ok(())
}

/// Writes and copies one length-delimited PHP byte string into the request byte section.
fn emit_string_value(
    ctx: &mut FunctionContext<'_>,
    value_offset: usize,
) -> Result<()> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, ptr_reg, TEMP_RESULT_LO_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, len_reg, TEMP_RESULT_HI_OFFSET);
    let request_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let cursor_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &request_reg, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &cursor_reg, 32);
    emit_store_u32_immediate(ctx, &request_reg, value_offset, 4);
    emit_store_u32_immediate(ctx, &request_reg, value_offset + 4, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &cursor_reg, 32);
    abi::emit_store_to_address(
        ctx.emitter,
        &cursor_reg,
        &request_reg,
        value_offset + 8,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, &cursor_reg, TEMP_RESULT_HI_OFFSET);
    abi::emit_store_to_address(
        ctx.emitter,
        &cursor_reg,
        &request_reg,
        value_offset + 16,
    );
    emit_copy_string_to_request(ctx)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, &cursor_reg, 32);
    let length_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &length_reg, TEMP_RESULT_HI_OFFSET);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("add {}, {}, {}", cursor_reg, cursor_reg, length_reg)); // advance the byte-section cursor by the copied string length
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("add {}, {}", cursor_reg, length_reg));    // advance the byte-section cursor by the copied string length
        }
    }
    abi::emit_store_to_sp(ctx.emitter, &cursor_reg, 32);
    Ok(())
}

/// Calls the target-aware request byte copier for one staged string payload.
fn emit_copy_string_to_request(
    ctx: &mut FunctionContext<'_>,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "x0",
                RUNTIME_VALUE_WRITE_CONTEXT_OFFSET + 24,
            );
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 32);
            ctx.emitter.instruction("add x0, x0, x9");                          // append after all previously copied string bytes
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", TEMP_RESULT_LO_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", TEMP_RESULT_HI_OFFSET);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "rdi",
                RUNTIME_VALUE_WRITE_CONTEXT_OFFSET + 24,
            );
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 32);
            ctx.emitter.instruction("add rdi, r10");                            // append after all previously copied string bytes
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", TEMP_RESULT_LO_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", TEMP_RESULT_HI_OFFSET);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_dom_copy_bytes");
    Ok(())
}

/// Writes a native-wrapper argument as an opaque bridge handle.
fn emit_object_value(
    ctx: &mut FunctionContext<'_>,
    argument: ValueId,
    value_offset: usize,
) -> Result<()> {
    emit_wrapper_object_pointer(ctx, argument)?;
    emit_object_pointer_value(ctx, value_offset)
}

/// Writes the current concrete native-wrapper object as an opaque bridge handle.
fn emit_object_pointer_value(
    ctx: &mut FunctionContext<'_>,
    value_offset: usize,
) -> Result<()> {
    let object_reg = abi::int_result_reg(ctx.emitter).to_string();
    let hidden_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    emit_wrapper_hidden_address(ctx, &object_reg, &hidden_reg);
    let handle_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_from_address(ctx.emitter, &handle_reg, &hidden_reg, 32);
    abi::emit_store_to_sp(ctx.emitter, &handle_reg, TEMP_RESULT_LO_OFFSET);
    let request_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &request_reg, 0);
    emit_store_u32_immediate(ctx, &request_reg, value_offset, 8);
    emit_store_u32_immediate(ctx, &request_reg, value_offset + 4, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &handle_reg, TEMP_RESULT_LO_OFFSET);
    abi::emit_store_to_address(
        ctx.emitter,
        &handle_reg,
        &request_reg,
        value_offset + 8,
    );
    abi::emit_store_zero_to_address(ctx.emitter, &request_reg, value_offset + 16);
    Ok(())
}

/// Locates compiler-hidden native-wrapper metadata from the object's runtime class ID.
fn emit_wrapper_hidden_address(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    hidden_reg: &str,
) {
    let class_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_from_address(ctx.emitter, &class_reg, object_reg, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let class_in_range = ctx.next_label("dom_wrapper_class_in_range");
            let has_metadata = ctx.next_label("dom_wrapper_class_has_metadata");
            abi::emit_load_symbol_to_reg(
                ctx.emitter,
                hidden_reg,
                "_class_internal_extension_hidden_offsets_count",
                0,
            );
            ctx.emitter
                .instruction(&format!("cmp {}, {}", class_reg, hidden_reg));    // is the runtime class represented in the native-wrapper offset table?
            ctx.emitter
                .instruction(&format!("b.lo {}", class_in_range));              // continue for an in-range runtime class identifier
            ctx.emitter.instruction("b __rt_dom_bridge_failure");               // reject a malformed or ordinary-object runtime class
            ctx.emitter.label(&class_in_range);
            abi::emit_symbol_address(
                ctx.emitter,
                hidden_reg,
                "_class_internal_extension_hidden_offsets",
            );
            ctx.emitter.instruction(&format!(
                "ldr {}, [{}, {}, lsl #3]",
                hidden_reg, hidden_reg, class_reg
            )); // load this concrete class's compiler-hidden metadata offset
            ctx.emitter
                .instruction(&format!("cbnz {}, {}", hidden_reg, has_metadata)); // continue only for a class with native wrapper metadata
            ctx.emitter.instruction("b __rt_dom_bridge_failure");               // reject a class without native wrapper metadata
            ctx.emitter.label(&has_metadata);
            ctx.emitter.instruction(&format!(
                "add {}, {}, {}",
                hidden_reg, object_reg, hidden_reg
            )); // address the concrete wrapper's compiler-hidden metadata
        }
        Arch::X86_64 => {
            abi::emit_cmp_reg_to_symbol(
                ctx.emitter,
                &class_reg,
                "_class_internal_extension_hidden_offsets_count",
            );
            ctx.emitter.instruction("jae __rt_dom_bridge_failure");             // reject a malformed or ordinary-object runtime class
            abi::emit_symbol_address(
                ctx.emitter,
                hidden_reg,
                "_class_internal_extension_hidden_offsets",
            );
            ctx.emitter.instruction(&format!(
                "mov {}, QWORD PTR [{} + {} * 8]",
                hidden_reg, hidden_reg, class_reg
            )); // load this concrete class's compiler-hidden metadata offset
            ctx.emitter
                .instruction(&format!("test {}, {}", hidden_reg, hidden_reg));  // does the concrete class own native wrapper metadata?
            ctx.emitter.instruction("jz __rt_dom_bridge_failure");              // reject a class without native wrapper metadata
            ctx.emitter.instruction(&format!(
                "add {}, {}",
                hidden_reg, object_reg
            )); // address the concrete wrapper's compiler-hidden metadata
        }
    }
}

/// Reports whether one public-void operation eagerly replaces SimpleXML iterator data.
fn simplexml_iterator_move_opcode(opcode: u32) -> bool {
    matches!(opcode, SIMPLEXML_NEXT_OPCODE | SIMPLEXML_REWIND_OPCODE)
}

/// Snapshots the receiver's mutation epoch before releasing its current wrapper.
fn emit_snapshot_simplexml_iterator_epoch(ctx: &mut FunctionContext<'_>) {
    let object = abi::int_result_reg(ctx.emitter).to_string();
    let epoch = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let hidden = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &object,
        RECEIVER_OBJECT_OFFSET,
    );
    emit_wrapper_hidden_address(ctx, &object, &hidden);
    abi::emit_load_from_address(
        ctx.emitter,
        &epoch,
        &hidden,
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_EPOCH_OFFSET,
    );
    abi::emit_store_to_sp(ctx.emitter, &epoch, RECEIVER_ITERATOR_EPOCH_OFFSET);
}

/// Skips a stale outer move when a destructor re-entered and mutated the iterator.
fn emit_branch_if_simplexml_iterator_epoch_changed(
    ctx: &mut FunctionContext<'_>,
    changed_label: &str,
) {
    let object = abi::int_result_reg(ctx.emitter).to_string();
    let epoch = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let hidden = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &object,
        RECEIVER_OBJECT_OFFSET,
    );
    emit_wrapper_hidden_address(ctx, &object, &hidden);
    abi::emit_load_from_address(
        ctx.emitter,
        &epoch,
        &hidden,
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_EPOCH_OFFSET,
    );
    let prior = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &prior,
        RECEIVER_ITERATOR_EPOCH_OFFSET,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", epoch, prior));            // did userland mutate iterator data during destruction?
            ctx.emitter
                .instruction(&format!("b.ne {}", changed_label));              // preserve the complete re-entrant move and skip this stale call
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", epoch, prior));            // did userland mutate iterator data during destruction?
            ctx.emitter
                .instruction(&format!("jne {}", changed_label));               // preserve the complete re-entrant move and skip this stale call
        }
    }
}

/// Advances the receiver's iterator mutation epoch after one successful native change.
fn emit_increment_simplexml_iterator_epoch(ctx: &mut FunctionContext<'_>) {
    let object = abi::int_result_reg(ctx.emitter).to_string();
    let epoch = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let hidden = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &object,
        RECEIVER_OBJECT_OFFSET,
    );
    emit_wrapper_hidden_address(ctx, &object, &hidden);
    abi::emit_load_from_address(
        ctx.emitter,
        &epoch,
        &hidden,
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_EPOCH_OFFSET,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("add {}, {}, #1", epoch, epoch));        // record one completed SimpleXML iterator mutation
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("add {}, 1", epoch));                    // record one completed SimpleXML iterator mutation
        }
    }
    abi::emit_store_to_address(
        ctx.emitter,
        &epoch,
        &hidden,
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_EPOCH_OFFSET,
    );
}

/// Clears and releases the PHP wrapper strongly retained as this iterator's current data.
///
/// The slot is cleared before the decrement so a user destructor can re-enter
/// the iterator without recursively releasing the same wrapper. For `next()`
/// and `rewind()` this runs before the native move, matching php-src's order:
/// the destructor still observes the old native iterator data.
fn emit_clear_simplexml_iterator_current_owner(ctx: &mut FunctionContext<'_>) {
    let object = abi::int_result_reg(ctx.emitter).to_string();
    let current = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let hidden = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    let done = ctx.next_label("simplexml_iterator_current_release_done");
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &object,
        RECEIVER_OBJECT_OFFSET,
    );
    emit_wrapper_hidden_address(ctx, &object, &hidden);
    abi::emit_load_from_address(
        ctx.emitter,
        &current,
        &hidden,
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_CURRENT_OFFSET,
    );
    abi::emit_store_zero_to_address(
        ctx.emitter,
        &hidden,
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_CURRENT_OFFSET,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz {}, {}", current, done));           // skip an iterator that has no eager current wrapper
            ctx.emitter
                .instruction(&format!("mov x0, {}", current));                 // transfer the released strong owner to object decref
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", current, current));       // does this iterator retain a current PHP wrapper?
            ctx.emitter
                .instruction(&format!("jz {}", done));                         // skip an iterator with no eager current wrapper
            ctx.emitter
                .instruction(&format!("mov rax, {}", current));                // transfer the released strong owner to object decref
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_decref_object");
    ctx.emitter.label(&done);
}

/// Transfers one owned materialized wrapper into the receiver's hidden iterator-data slot.
fn emit_store_simplexml_iterator_current_owner(ctx: &mut FunctionContext<'_>) {
    let object = abi::int_result_reg(ctx.emitter).to_string();
    let current = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let hidden = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    let empty = ctx.next_label("simplexml_iterator_current_replace_empty");
    abi::emit_store_to_sp(ctx.emitter, &object, TEMP_RESULT_LO_OFFSET);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &object,
        RECEIVER_OBJECT_OFFSET,
    );
    emit_wrapper_hidden_address(ctx, &object, &hidden);
    abi::emit_load_from_address(
        ctx.emitter,
        &current,
        &hidden,
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_CURRENT_OFFSET,
    );
    abi::emit_store_zero_to_address(
        ctx.emitter,
        &hidden,
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_CURRENT_OFFSET,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbz {}, {}", current, empty));          // skip replacement cleanup for the expected empty slot
            ctx.emitter
                .instruction(&format!("mov x0, {}", current));                 // release an owner installed by unexpected re-entry
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", current, current));       // did re-entry install an owner before replacement?
            ctx.emitter
                .instruction(&format!("jz {}", empty));                        // skip replacement cleanup for the expected empty slot
            ctx.emitter
                .instruction(&format!("mov rax, {}", current));                // release an owner installed by unexpected re-entry
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_decref_object");
    ctx.emitter.label(&empty);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &object,
        RECEIVER_OBJECT_OFFSET,
    );
    emit_wrapper_hidden_address(ctx, &object, &hidden);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &current,
        TEMP_RESULT_LO_OFFSET,
    );
    abi::emit_store_to_address(
        ctx.emitter,
        &current,
        &hidden,
        crate::internal_extensions::NATIVE_WRAPPER_ITERATOR_CURRENT_OFFSET,
    );
}

/// Invokes `elephc_dom_call` after zero-initializing the fixed result header.
fn emit_native_call(
    ctx: &mut FunctionContext<'_>,
    instruction: &Instruction,
    opcode: u32,
) -> Result<()> {
    let result_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_temporary_stack_address(ctx.emitter, &result_reg, RESULT_FRAME_OFFSET);
    for offset in (0..RESULT_HEADER_SIZE).step_by(8) {
        abi::emit_store_zero_to_address(ctx.emitter, &result_reg, offset);
    }
    let symbol = ctx.emitter.target.extern_symbol("elephc_dom_call");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            abi::emit_temporary_stack_address(ctx.emitter, "x3", RESULT_FRAME_OFFSET);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", 8);
            abi::emit_temporary_stack_address(ctx.emitter, "rcx", RESULT_FRAME_OFFSET);
        }
    }
    abi::emit_call_label(ctx.emitter, &symbol);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let call_status_ok = ctx.next_label("dom_call_status_ok");
            let call_panicked = ctx.next_label("dom_call_panicked");
            let result_status_valid = ctx.next_label("dom_result_status_valid");
            let result_status_ok = ctx.next_label("dom_result_status_ok");
            let result_dom_exception = ctx.next_label("dom_result_dom_exception");
            let result_value_error = ctx.next_label("dom_result_value_error");
            let result_type_error = ctx.next_label("dom_result_type_error");
            let result_error = ctx.next_label("dom_result_error");
            let result_exception = ctx.next_label("dom_result_exception");
            let result_pending_host_throwable =
                ctx.next_label("dom_result_pending_host_throwable");
            ctx.emitter
                .instruction(&format!("cbz w0, {}", call_status_ok));            // continue only when the native entry point reports success
            ctx.emitter.instruction("cmp w0, #4");                              // did the native boundary contain an internal panic?
            ctx.emitter
                .instruction(&format!("b.eq {}", call_panicked));               // classify contained panics independently from malformed ABI requests
            ctx.emitter.instruction("mov x0, #71");                             // classify a failed native entry-point status
            ctx.emitter.instruction("b __rt_dom_bridge_failure_code");          // contain the native entry-point failure
            ctx.emitter.label(&call_panicked);
            ctx.emitter.instruction("mov x0, #74");                             // classify a contained native panic
            ctx.emitter.instruction("b __rt_dom_bridge_failure_code");          // terminate without unwinding across the ABI
            ctx.emitter.label(&call_status_ok);
            let result_header_invalid = ctx.next_label("dom_result_header_invalid");
            let result_header_ready = ctx.next_label("dom_result_header_ready");
            ctx.emitter.instruction("ldr w9, [sp, #48]");                       // load the returned ABI-version discriminator
            ctx.emitter.instruction(&format!("cmp w9, #{}", ABI_VERSION));      // does the result use the compiler's frozen ABI version?
            ctx.emitter
                .instruction(&format!("b.ne {}", result_header_invalid));       // reject a stale result ABI before reading any payload field
            ctx.emitter.instruction("ldr w9, [sp, #52]");                       // load the returned fixed-header size
            ctx.emitter
                .instruction(&format!("cmp w9, #{}", RESULT_HEADER_SIZE));      // is the complete result header present?
            ctx.emitter
                .instruction(&format!("b.eq {}", result_header_ready));         // continue only for the exact frozen header layout
            ctx.emitter.label(&result_header_invalid);
            ctx.emitter.instruction("mov x0, #75");                             // classify an incompatible result header
            ctx.emitter.instruction("b __rt_dom_bridge_failure_code");          // reject stale or truncated results before reading pointers
            ctx.emitter.label(&result_header_ready);
            ctx.emitter.instruction("ldr w9, [sp, #56]");                       // load the primary bridge result status
            ctx.emitter
                .instruction(&format!("cbz w9, {}", result_status_valid));       // accept a successful primary result
            ctx.emitter.instruction("cmp w9, #1");                              // is the primary result a catchable PHP exception?
            ctx.emitter
                .instruction(&format!("b.eq {}", result_status_valid));         // accept a structured PHP exception result
            ctx.emitter.instruction("mov x0, #72");                             // classify a non-success primary result status
            ctx.emitter.instruction("b __rt_dom_bridge_failure_code");          // structured translation is required before continuing
            ctx.emitter.label(&result_status_valid);
            emit_result_diagnostics(ctx, instruction, opcode);
            ctx.emitter.instruction("ldr w9, [sp, #56]");                       // reload the validated primary result status
            ctx.emitter
                .instruction(&format!("cbz w9, {}", result_status_ok));          // materialize ordinary successful results after diagnostics
            ctx.emitter.instruction("ldr w9, [sp, #64]");                       // load the structured PHP exception-kind discriminator
            ctx.emitter.instruction("cmp w9, #1");                              // is this the declared DOMException result kind?
            ctx.emitter
                .instruction(&format!("b.eq {}", result_dom_exception));        // materialize the catchable DOMException object
            ctx.emitter.instruction("cmp w9, #2");                              // is this the declared ValueError result kind?
            ctx.emitter
                .instruction(&format!("b.eq {}", result_value_error));          // materialize the catchable ValueError object
            ctx.emitter.instruction("cmp w9, #3");                              // is this the declared base Error result kind?
            ctx.emitter
                .instruction(&format!("b.eq {}", result_error));                // materialize the catchable base Error object
            ctx.emitter.instruction("cmp w9, #4");                              // is this the declared base Exception result kind?
            ctx.emitter
                .instruction(&format!("b.eq {}", result_exception));            // materialize the catchable base Exception object
            ctx.emitter.instruction("cmp w9, #5");                              // is this the declared TypeError result kind?
            ctx.emitter
                .instruction(&format!("b.eq {}", result_type_error));           // materialize the catchable TypeError object
            ctx.emitter.instruction("cmp w9, #6");                              // did a PHP host callback preserve its original Throwable?
            ctx.emitter.instruction(&format!(
                "b.eq {}",
                result_pending_host_throwable
            ));                                                                 // rethrow the exact pending host object after native cleanup
            ctx.emitter.instruction("mov x0, #72");                             // classify an unknown structured exception kind
            ctx.emitter.instruction("b __rt_dom_bridge_failure_code");          // reject a result outside the bridge exception contract
            ctx.emitter.label(&result_dom_exception);
            super::exceptions::emit_dom_exception_from_result(ctx, 96, 88, 68);
            ctx.emitter.label(&result_value_error);
            super::exceptions::emit_value_error_from_result(ctx, 96, 88);
            ctx.emitter.label(&result_type_error);
            super::exceptions::emit_type_error_from_result(ctx, 96, 88);
            ctx.emitter.label(&result_error);
            super::exceptions::emit_error_from_result(ctx, 96, 88);
            ctx.emitter.label(&result_exception);
            super::exceptions::emit_exception_from_result(ctx, 96, 88);
            ctx.emitter.label(&result_pending_host_throwable);
            emit_pending_host_throwable(ctx)?;
            ctx.emitter.label(&result_status_ok);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test eax, eax");                           // did the native entry point report success?
            let call_status_ok = ctx.next_label("dom_call_status_ok");
            let call_panicked = ctx.next_label("dom_call_panicked");
            ctx.emitter
                .instruction(&format!("jz {}", call_status_ok));                // continue only when the native entry point reports success
            ctx.emitter.instruction("cmp eax, 4");                              // did the native boundary contain an internal panic?
            ctx.emitter
                .instruction(&format!("je {}", call_panicked));                 // classify contained panics independently from malformed ABI requests
            ctx.emitter.instruction("mov eax, 71");                             // classify a failed native entry-point status
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure_code");        // contain the native entry-point failure
            ctx.emitter.label(&call_panicked);
            ctx.emitter.instruction("mov eax, 74");                             // classify a contained native panic
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure_code");        // terminate without unwinding across the ABI
            ctx.emitter.label(&call_status_ok);
            let result_header_invalid = ctx.next_label("dom_result_header_invalid");
            let result_header_ready = ctx.next_label("dom_result_header_ready");
            ctx.emitter
                .instruction(&format!("cmp DWORD PTR [rsp + 48], {}", ABI_VERSION)); // does the result use the compiler's frozen ABI version?
            ctx.emitter
                .instruction(&format!("jne {}", result_header_invalid));        // reject a stale result ABI before reading any payload field
            ctx.emitter.instruction(&format!(
                "cmp DWORD PTR [rsp + 52], {}",
                RESULT_HEADER_SIZE
            ));                                                                 // require the complete fixed result-header layout
            ctx.emitter
                .instruction(&format!("je {}", result_header_ready));           // continue only for the exact frozen header size
            ctx.emitter.label(&result_header_invalid);
            ctx.emitter.instruction("mov eax, 75");                             // classify an incompatible result header
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure_code");        // reject stale or truncated results before reading pointers
            ctx.emitter.label(&result_header_ready);
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 56], 0");             // did the primary result report a PHP/native failure?
            let result_status_valid = ctx.next_label("dom_result_status_valid");
            let result_status_ok = ctx.next_label("dom_result_status_ok");
            let result_dom_exception = ctx.next_label("dom_result_dom_exception");
            let result_value_error = ctx.next_label("dom_result_value_error");
            let result_type_error = ctx.next_label("dom_result_type_error");
            let result_error = ctx.next_label("dom_result_error");
            let result_exception = ctx.next_label("dom_result_exception");
            let result_pending_host_throwable =
                ctx.next_label("dom_result_pending_host_throwable");
            ctx.emitter
                .instruction(&format!("je {}", result_status_valid));           // accept a successful primary result
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 56], 1");             // is the primary result a catchable PHP exception?
            ctx.emitter
                .instruction(&format!("je {}", result_status_valid));           // accept a structured PHP exception result
            ctx.emitter.instruction("mov eax, 72");                             // classify a non-success primary result status
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure_code");        // structured failures never masquerade as successful values
            ctx.emitter.label(&result_status_valid);
            emit_result_diagnostics(ctx, instruction, opcode);
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 56], 0");             // reload the validated primary result status
            ctx.emitter
                .instruction(&format!("je {}", result_status_ok));              // materialize ordinary successful results after diagnostics
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 64], 1");             // is this the declared DOMException result kind?
            ctx.emitter
                .instruction(&format!("je {}", result_dom_exception));          // materialize the catchable DOMException object
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 64], 2");             // is this the declared ValueError result kind?
            ctx.emitter
                .instruction(&format!("je {}", result_value_error));            // materialize the catchable ValueError object
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 64], 3");             // is this the declared base Error result kind?
            ctx.emitter
                .instruction(&format!("je {}", result_error));                  // materialize the catchable base Error object
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 64], 4");             // is this the declared base Exception result kind?
            ctx.emitter
                .instruction(&format!("je {}", result_exception));              // materialize the catchable base Exception object
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 64], 5");             // is this the declared TypeError result kind?
            ctx.emitter
                .instruction(&format!("je {}", result_type_error));              // materialize the catchable TypeError object
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 64], 6");             // did a PHP host callback preserve its original Throwable?
            ctx.emitter.instruction(&format!(
                "je {}",
                result_pending_host_throwable
            ));                                                                 // rethrow the exact pending host object after native cleanup
            ctx.emitter.instruction("mov eax, 72");                             // classify an unknown structured exception kind
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure_code");        // reject a result outside the bridge exception contract
            ctx.emitter.label(&result_dom_exception);
            super::exceptions::emit_dom_exception_from_result(ctx, 96, 88, 68);
            ctx.emitter.label(&result_value_error);
            super::exceptions::emit_value_error_from_result(ctx, 96, 88);
            ctx.emitter.label(&result_type_error);
            super::exceptions::emit_type_error_from_result(ctx, 96, 88);
            ctx.emitter.label(&result_error);
            super::exceptions::emit_error_from_result(ctx, 96, 88);
            ctx.emitter.label(&result_exception);
            super::exceptions::emit_exception_from_result(ctx, 96, 88);
            ctx.emitter.label(&result_pending_host_throwable);
            emit_pending_host_throwable(ctx)?;
            ctx.emitter.label(&result_status_ok);
        }
    }
    Ok(())
}

/// Releases bridge-owned call state before rethrowing a Throwable captured by the host sentinel.
fn emit_pending_host_throwable(ctx: &mut FunctionContext<'_>) -> Result<()> {
    emit_release_native_call_state(ctx)?;
    abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
    abi::emit_jump(ctx.emitter, "__rt_throw_current");
    Ok(())
}

/// Builds PHP CLI warning fragments for one source-backed bridge call site.
fn callsite_warning_fragments(
    ctx: &FunctionContext<'_>,
    instruction: &Instruction,
) -> (String, String) {
    let Some(source_path) = ctx.module.source_path.as_deref() else {
        return ("\nWarning: Unknown: ".to_string(), "\n".to_string());
    };
    let Some(span) = instruction.span.filter(|span| span.line > 0) else {
        return ("\nWarning: Unknown: ".to_string(), "\n".to_string());
    };
    let callable = ctx.function.name.trim_start_matches('\\');
    (
        format!("\nWarning: {callable}(): "),
        format!(" in {source_path} on line {}\n", span.line),
    )
}

/// Emits ordered bridge diagnostics after validating their retained byte ranges.
fn emit_result_diagnostics(
    ctx: &mut FunctionContext<'_>,
    instruction: &Instruction,
    opcode: u32,
) {
    let loop_label = ctx.next_label("dom_result_diagnostic_loop");
    let done = ctx.next_label("dom_result_diagnostic_done");
    let invalid = ctx.next_label("dom_result_diagnostic_invalid");
    let callsite_fragments = match opcode {
        SIMPLEXML_READ_DIMENSION_OPCODE | SIMPLEXML_WRITE_DIMENSION_OPCODE => {
            let (prefix, suffix) = callsite_warning_fragments(ctx, instruction);
            Some((DIAGNOSTIC_FLAG_CALLSITE_CONTEXT, prefix, suffix))
        }
        LEGACY_DOM_APPEND_CHILD_OPCODE
        | SIMPLEXML_ADD_ATTRIBUTE_OPCODE
        | SIMPLEXML_ADD_CHILD_OPCODE
        | SIMPLEXML_XPATH_OPCODE => {
            let (_, suffix) = callsite_warning_fragments(ctx, instruction);
            Some((
                DIAGNOSTIC_FLAG_CALLSITE_LOCATION,
                "\n".to_string(),
                suffix,
            ))
        }
        _ => None,
    }
    .map(|(expected_flag, prefix, suffix)| {
        let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
        let (suffix_label, suffix_len) = ctx.data.add_string(suffix.as_bytes());
        (
            expected_flag,
            prefix_label,
            prefix_len,
            suffix_label,
            suffix_len,
        )
    });
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #136]");                      // load the retained diagnostic record count
            ctx.emitter
                .instruction(&format!("cbz x9, {}", done));                     // skip the diagnostic loop for an empty result range
            ctx.emitter.instruction("ldr x10, [sp, #128]");                     // load the retained diagnostic record pointer
            ctx.emitter
                .instruction(&format!("cbz x10, {}", invalid));                 // reject a non-empty range with a null record pointer
            ctx.emitter.instruction("str x10, [sp, #144]");                     // stage the current diagnostic record pointer across writes
            ctx.emitter.instruction("str x9, [sp, #152]");                      // stage the remaining diagnostic count across writes
            ctx.emitter.label(&loop_label);
            ctx.emitter.instruction("ldr x10, [sp, #144]");                     // reload the current diagnostic record
            ctx.emitter.instruction("ldr x11, [x10, #32]");                     // load its message offset into the result byte buffer
            ctx.emitter.instruction("ldr x12, [x10, #40]");                     // load its exact message byte length
            ctx.emitter.instruction("ldr x13, [sp, #104]");                     // load the complete retained result byte length
            ctx.emitter.instruction("cmp x11, x13");                            // is the diagnostic message offset in bounds?
            ctx.emitter
                .instruction(&format!("b.hi {}", invalid));                     // reject an out-of-bounds diagnostic message offset
            ctx.emitter.instruction("sub x13, x13, x11");                       // compute the bytes remaining after the message offset
            ctx.emitter.instruction("cmp x12, x13");                            // does the complete message fit in the retained bytes?
            ctx.emitter
                .instruction(&format!("b.hi {}", invalid));                     // reject an out-of-bounds diagnostic message length
            ctx.emitter.instruction("ldr x1, [sp, #96]");                       // load the retained result byte-buffer pointer
            ctx.emitter
                .instruction(&format!("cbz x1, {}", invalid));                  // non-empty diagnostic messages require retained bytes
            ctx.emitter.instruction("add x1, x1, x11");                         // address the exact diagnostic message byte range
            ctx.emitter.instruction("mov x2, x12");                             // pass its byte length to the warning channel
            if let Some((
                expected_flag,
                prefix_label,
                prefix_len,
                suffix_label,
                suffix_len,
            )) = &callsite_fragments
            {
                let raw = ctx.next_label("dom_result_diagnostic_raw");
                let emitted = ctx.next_label("dom_result_diagnostic_emitted");
                ctx.emitter.instruction("ldr w13, [x10, #12]");                 // load the diagnostic decoration flags
                ctx.emitter
                    .instruction(&format!("cbz w13, {}", raw));                // preserve already formatted bridge diagnostics
                ctx.emitter.instruction(&format!(
                    "cmp w13, #{}",
                    expected_flag
                ));                                                             // require this operation's call-site decoration mode
                ctx.emitter
                    .instruction(&format!("b.ne {}", invalid));                // reject unknown diagnostic flag combinations
                ctx.emitter.instruction("mov x3, x1");                          // preserve the dynamic warning detail pointer
                ctx.emitter.instruction("mov x4, x2");                          // preserve the dynamic warning detail length
                abi::emit_symbol_address(ctx.emitter, "x1", prefix_label);
                abi::emit_load_int_immediate(ctx.emitter, "x2", *prefix_len as i64);
                abi::emit_call_label(ctx.emitter, "__rt_concat");              // prepend the PHP callable warning prefix
                abi::emit_symbol_address(ctx.emitter, "x3", suffix_label);
                abi::emit_load_int_immediate(ctx.emitter, "x4", *suffix_len as i64);
                abi::emit_call_label(ctx.emitter, "__rt_concat");              // append the exact source path and line
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning");        // emit or suppress one complete PHP warning
                ctx.emitter
                    .instruction(&format!("b {}", emitted));                   // avoid re-emitting the raw warning detail
                ctx.emitter.label(&raw);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning");        // emit an already formatted bridge diagnostic
                ctx.emitter.label(&emitted);
            } else {
                ctx.emitter.instruction("ldr w13, [x10, #12]");                 // load the diagnostic decoration flags
                ctx.emitter
                    .instruction(&format!("cbnz w13, {}", invalid));           // reject call-site flags on unsupported operations
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning");        // emit an already formatted bridge diagnostic
            }
            ctx.emitter.instruction("ldr x10, [sp, #144]");                     // restore the current diagnostic record pointer
            ctx.emitter.instruction(&format!(
                "add x10, x10, #{}",
                ABI_DIAGNOSTIC_SIZE
            )); // advance to the next fixed-width diagnostic record
            ctx.emitter.instruction("str x10, [sp, #144]");                     // persist the advanced diagnostic record pointer
            ctx.emitter.instruction("ldr x9, [sp, #152]");                      // restore the remaining diagnostic count
            ctx.emitter.instruction("subs x9, x9, #1");                         // consume the diagnostic that was just emitted
            ctx.emitter.instruction("str x9, [sp, #152]");                      // persist the remaining count across the next write
            ctx.emitter
                .instruction(&format!("b.ne {}", loop_label));                  // continue until every ordered diagnostic is emitted
            ctx.emitter
                .instruction(&format!("b {}", done));                           // skip the malformed-result containment path
            ctx.emitter.label(&invalid);
            ctx.emitter.instruction("mov x0, #72");                             // classify a malformed native diagnostic range
            ctx.emitter.instruction("b __rt_dom_bridge_failure_code");          // contain the malformed native result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 136]");          // load the retained diagnostic record count
            ctx.emitter.instruction("test rax, rax");                           // is the diagnostic range empty?
            ctx.emitter
                .instruction(&format!("jz {}", done));                          // skip the diagnostic loop for an empty result range
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 128]");          // load the retained diagnostic record pointer
            ctx.emitter.instruction("test r10, r10");                           // is the non-empty record range backed by storage?
            ctx.emitter
                .instruction(&format!("jz {}", invalid));                       // reject a non-empty range with a null record pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 144], r10");          // stage the current diagnostic record pointer across writes
            ctx.emitter.instruction("mov QWORD PTR [rsp + 152], rax");          // stage the remaining diagnostic count across writes
            ctx.emitter.label(&loop_label);
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 144]");          // reload the current diagnostic record
            ctx.emitter.instruction("mov r11, QWORD PTR [r10 + 32]");           // load its message offset into the result byte buffer
            ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 104]");          // load the complete retained result byte length
            ctx.emitter.instruction("cmp r11, rcx");                            // is the diagnostic message offset in bounds?
            ctx.emitter
                .instruction(&format!("ja {}", invalid));                       // reject an out-of-bounds diagnostic message offset
            ctx.emitter.instruction("sub rcx, r11");                            // compute the bytes remaining after the message offset
            ctx.emitter.instruction("mov rax, QWORD PTR [r10 + 40]");           // load the exact diagnostic message byte length
            ctx.emitter.instruction("cmp rax, rcx");                            // does the complete message fit in the retained bytes?
            ctx.emitter
                .instruction(&format!("ja {}", invalid));                       // reject an out-of-bounds diagnostic message length
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 96]");           // load the retained result byte-buffer pointer
            ctx.emitter.instruction("test rdi, rdi");                           // do the diagnostic message bytes have storage?
            ctx.emitter
                .instruction(&format!("jz {}", invalid));                       // non-empty diagnostic messages require retained bytes
            ctx.emitter.instruction("add rdi, r11");                            // address the exact diagnostic message byte range
            ctx.emitter.instruction("mov rsi, rax");                            // pass its byte length to the warning channel
            if let Some((
                expected_flag,
                prefix_label,
                prefix_len,
                suffix_label,
                suffix_len,
            )) = &callsite_fragments
            {
                let raw = ctx.next_label("dom_result_diagnostic_raw");
                let emitted = ctx.next_label("dom_result_diagnostic_emitted");
                ctx.emitter.instruction("mov ecx, DWORD PTR [r10 + 12]");       // load the diagnostic decoration flags
                ctx.emitter.instruction("test ecx, ecx");                       // is the diagnostic already fully formatted?
                ctx.emitter
                    .instruction(&format!("jz {}", raw));                      // preserve already formatted bridge diagnostics
                ctx.emitter.instruction(&format!(
                    "cmp ecx, {}",
                    expected_flag
                ));                                                             // require this operation's call-site decoration mode
                ctx.emitter
                    .instruction(&format!("jne {}", invalid));                 // reject unknown diagnostic flag combinations
                abi::emit_symbol_address(ctx.emitter, "rax", prefix_label);
                abi::emit_load_int_immediate(ctx.emitter, "rdx", *prefix_len as i64);
                abi::emit_call_label(ctx.emitter, "__rt_concat");              // prepend the PHP callable warning prefix
                abi::emit_symbol_address(ctx.emitter, "rdi", suffix_label);
                abi::emit_load_int_immediate(ctx.emitter, "rsi", *suffix_len as i64);
                abi::emit_call_label(ctx.emitter, "__rt_concat");              // append the exact source path and line
                ctx.emitter.instruction("mov rdi, rax");                        // pass the complete decorated warning pointer
                ctx.emitter.instruction("mov rsi, rdx");                        // pass the complete decorated warning length
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning");        // emit or suppress one complete PHP warning
                ctx.emitter
                    .instruction(&format!("jmp {}", emitted));                 // avoid re-emitting the raw warning detail
                ctx.emitter.label(&raw);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning");        // emit an already formatted bridge diagnostic
                ctx.emitter.label(&emitted);
            } else {
                ctx.emitter.instruction("mov ecx, DWORD PTR [r10 + 12]");       // load the diagnostic decoration flags
                ctx.emitter.instruction("test ecx, ecx");                       // is the diagnostic free of unsupported flags?
                ctx.emitter
                    .instruction(&format!("jnz {}", invalid));                 // reject call-site flags on unsupported operations
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning");        // emit an already formatted bridge diagnostic
            }
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 144]");          // restore the current diagnostic record pointer
            ctx.emitter.instruction(&format!(
                "add r10, {}",
                ABI_DIAGNOSTIC_SIZE
            )); // advance to the next fixed-width diagnostic record
            ctx.emitter.instruction("mov QWORD PTR [rsp + 144], r10");          // persist the advanced diagnostic record pointer
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 152]");          // restore the remaining diagnostic count
            ctx.emitter.instruction("sub rax, 1");                              // consume the diagnostic that was just emitted
            ctx.emitter.instruction("mov QWORD PTR [rsp + 152], rax");          // persist the remaining count across the next write
            ctx.emitter
                .instruction(&format!("jnz {}", loop_label));                   // continue until every ordered diagnostic is emitted
            ctx.emitter
                .instruction(&format!("jmp {}", done));                         // skip the malformed-result containment path
            ctx.emitter.label(&invalid);
            ctx.emitter.instruction("mov eax, 72");                             // classify a malformed native diagnostic range
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure_code");        // contain the malformed native result
        }
    }
    ctx.emitter.label(&done);
}

/// Materializes one ABI result into the EIR result representation and releases native frames.
fn emit_result(
    ctx: &mut FunctionContext<'_>,
    instruction: &Instruction,
    opcode: u32,
    flags: u32,
    result_contract: &PhpType,
) -> Result<()> {
    let expected = instruction.result_php_type.codegen_repr();
    match expected {
        PhpType::Object(class_name) => {
            if flags & FLAG_WRAPPER_RESULT != 0 {
                require_result_tag(ctx, 8)?;
                let eager_xpath_nodeset = matches!(
                    opcode,
                    MODERN_DOM_XPATH_QUERY_OPCODE | LEGACY_DOM_XPATH_QUERY_OPCODE
                );
                if eager_xpath_nodeset {
                    materialize_xpath_nodeset_members(ctx)?;
                }
                let context_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
                let handle_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
                abi::emit_load_temporary_stack_slot(ctx.emitter, &context_reg, 16);
                abi::emit_load_temporary_stack_slot(
                    ctx.emitter,
                    &handle_reg,
                    RESULT_FRAME_OFFSET + 32,
                );
                emit_typed_wrapper_result(
                    ctx,
                    &class_name,
                    &context_reg,
                    &handle_reg,
                    false,
                    Some(opcode),
                )?;
                if eager_xpath_nodeset {
                    attach_xpath_nodeset_owner(ctx);
                }
                store_instruction_result(ctx, instruction)?;
            } else if flags & FLAG_VALUE_OBJECT_RESULT != 0 {
                require_result_tag(ctx, 11)?;
                stage_result_word(
                    ctx,
                    RESULT_FRAME_OFFSET + 32,
                    OBJECT_FIELDS_OFFSET,
                );
                stage_result_word(
                    ctx,
                    RESULT_FRAME_OFFSET + 40,
                    OBJECT_FIELD_COUNT_OFFSET,
                );
                materialize_libxml_error_value_object(ctx)?;
                store_instruction_result(ctx, instruction)?;
            } else {
                return Err(CodegenIrError::invalid_module(
                    "internal-extension object result missing materialization flag",
                ));
            }
            emit_release_native_call_state(ctx)?;
            abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
        }
        PhpType::Bool | PhpType::False => {
            require_result_tag(ctx, 1)?;
            stage_result_word(ctx, RESULT_FRAME_OFFSET + 32, TEMP_RESULT_LO_OFFSET);
            emit_release_native_call_state(ctx)?;
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                TEMP_RESULT_LO_OFFSET,
            );
            abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
            store_instruction_result(ctx, instruction)?;
        }
        PhpType::Int => {
            require_result_tag(ctx, 2)?;
            stage_result_word(ctx, RESULT_FRAME_OFFSET + 32, TEMP_RESULT_LO_OFFSET);
            emit_release_native_call_state(ctx)?;
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                TEMP_RESULT_LO_OFFSET,
            );
            abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
            store_instruction_result(ctx, instruction)?;
        }
        PhpType::TaggedScalar => {
            materialize_tagged_scalar_result(ctx)?;
            emit_release_native_call_state(ctx)?;
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                TEMP_RESULT_LO_OFFSET,
            );
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
                TEMP_RESULT_HI_OFFSET,
            );
            abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
            store_instruction_result(ctx, instruction)?;
        }
        PhpType::Float => {
            require_result_tag(ctx, 3)?;
            stage_result_word(ctx, RESULT_FRAME_OFFSET + 32, TEMP_RESULT_LO_OFFSET);
            emit_release_native_call_state(ctx)?;
            let scratch = abi::secondary_scratch_reg(ctx.emitter).to_string();
            abi::emit_load_temporary_stack_slot(ctx.emitter, &scratch, TEMP_RESULT_LO_OFFSET);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter
                        .instruction(&format!("fmov {}, {}", abi::float_result_reg(ctx.emitter), scratch)); // restore the exact returned IEEE-754 bits
                }
                Arch::X86_64 => {
                    ctx.emitter
                        .instruction(&format!("movq {}, {}", abi::float_result_reg(ctx.emitter), scratch)); // restore the exact returned IEEE-754 bits
                }
            }
            abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
            store_instruction_result(ctx, instruction)?;
        }
        PhpType::Str => {
            materialize_owned_result_string(ctx)?;
            stage_current_string(ctx);
            emit_release_native_call_state(ctx)?;
            restore_current_string(ctx);
            abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
            store_instruction_result(ctx, instruction)?;
        }
        PhpType::Callable => {
            require_result_tag(ctx, 9)?;
            stage_result_word(ctx, RESULT_FRAME_OFFSET + 32, TEMP_RESULT_LO_OFFSET);
            emit_release_native_call_state(ctx)?;
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                TEMP_RESULT_LO_OFFSET,
            );
            abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
            store_instruction_result(ctx, instruction)?;
        }
        PhpType::Array(element_type) => {
            materialize_result_array(ctx, &element_type)?;
            abi::emit_store_to_sp(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                TEMP_RESULT_LO_OFFSET,
            );
            emit_release_native_call_state(ctx)?;
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                TEMP_RESULT_LO_OFFSET,
            );
            abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
            store_instruction_result(ctx, instruction)?;
        }
        PhpType::AssocArray { key, value } => {
            materialize_result_map(ctx, &key, &value)?;
            abi::emit_store_to_sp(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                TEMP_RESULT_LO_OFFSET,
            );
            emit_release_native_call_state(ctx)?;
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                TEMP_RESULT_LO_OFFSET,
            );
            abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
            store_instruction_result(ctx, instruction)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            materialize_mixed_result(ctx, opcode, flags, result_contract)?;
            abi::emit_store_to_sp(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                TEMP_RESULT_LO_OFFSET,
            );
            emit_release_native_call_state(ctx)?;
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                TEMP_RESULT_LO_OFFSET,
            );
            abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
            store_instruction_result(ctx, instruction)?;
        }
        PhpType::Void => {
            if simplexml_iterator_move_opcode(opcode) {
                materialize_simplexml_iterator_move_result(ctx, opcode)?;
            } else {
                require_result_tag(ctx, 0)?;
                if opcode == SIMPLEXML_CONSTRUCTOR_OPCODE {
                    emit_clear_simplexml_iterator_current_owner(ctx);
                    emit_increment_simplexml_iterator_epoch(ctx);
                }
            }
            emit_release_native_call_state(ctx)?;
            abi::emit_release_temporary_stack(ctx.emitter, CALL_FRAME_SIZE);
            if instruction.result.is_some() {
                abi::emit_load_int_immediate(
                    ctx.emitter,
                    abi::int_result_reg(ctx.emitter),
                    crate::codegen::NULL_SENTINEL,
                );
                store_instruction_result(ctx, instruction)?;
            }
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "internal extension result type {other:?}"
            )));
        }
    }
    Ok(())
}

/// Materializes php-src's eager XPath member wrappers before the `DOMNodeList`.
fn materialize_xpath_nodeset_members(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let capacity_ready = ctx.next_label("dom_xpath_eager_capacity_ready");
    let pointer_ready = ctx.next_label("dom_xpath_eager_pointer_ready");
    let loop_head = ctx.next_label("dom_xpath_eager_loop");
    let member = ctx.next_label("dom_xpath_eager_member");
    let member_without_parent =
        ctx.next_label("dom_xpath_eager_member_without_parent");
    let advance = ctx.next_label("dom_xpath_eager_advance");
    let loop_done = ctx.next_label("dom_xpath_eager_done");
    let failure = ctx.next_label("dom_xpath_eager_failure");

    stage_result_word(ctx, RESULT_FRAME_OFFSET + 72, ARRAY_COUNT_OFFSET);
    store_stack_immediate(ctx, ARRAY_INDEX_OFFSET, 0);
    store_stack_immediate(ctx, TEMP_RESULT_LO_OFFSET, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #120]");                    // load the eager wrapper descriptor count
            ctx.emitter.instruction("ldr x9, [sp, #112]");                    // load the retained descriptor-vector pointer
            ctx.emitter
                .instruction(&format!("cbz x0, {}", pointer_ready));          // an empty nodeset permits a null descriptor pointer
            ctx.emitter
                .instruction(&format!("cbz x9, {}", failure));                // non-empty nodesets require retained descriptors
            ctx.emitter.label(&pointer_ready);
            ctx.emitter.instruction("cmp x0, #4");                             // enforce the runtime minimum indexed-array capacity
            ctx.emitter
                .instruction(&format!("b.hs {}", capacity_ready));            // retain a sufficiently large descriptor count
            ctx.emitter.instruction("mov x0, #4");                             // raise small nodesets to minimum capacity
            ctx.emitter.label(&capacity_ready);
            ctx.emitter.instruction("mov x1, #8");                             // eager member arrays store object pointers
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 120]");         // load the eager wrapper descriptor count
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 112]");        // load the retained descriptor-vector pointer
            ctx.emitter.instruction("test rdi, rdi");                          // does the result contain eager descriptors?
            ctx.emitter
                .instruction(&format!("jz {}", pointer_ready));               // an empty nodeset permits a null descriptor pointer
            ctx.emitter.instruction("test r10, r10");                          // non-empty nodesets require retained descriptors
            ctx.emitter
                .instruction(&format!("jz {}", failure));                     // contain a malformed retained vector
            ctx.emitter.label(&pointer_ready);
            ctx.emitter.instruction("cmp rdi, 4");                             // enforce the runtime minimum indexed-array capacity
            ctx.emitter
                .instruction(&format!("jae {}", capacity_ready));             // retain a sufficiently large descriptor count
            ctx.emitter.instruction("mov rdi, 4");                             // raise small nodesets to minimum capacity
            ctx.emitter.label(&capacity_ready);
            ctx.emitter.instruction("mov rsi, 8");                             // eager member arrays store object pointers
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Object("DOMNode".to_string()),
    );
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        ARRAY_RESULT_OFFSET,
    );

    ctx.emitter.label(&loop_head);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #168]");                    // load the next eager descriptor index
            ctx.emitter.instruction("ldr x10, [sp, #176]");                   // load the descriptor count
            ctx.emitter.instruction("cmp x9, x10");                            // have all wrappers been materialized?
            ctx.emitter
                .instruction(&format!("b.hs {}", loop_done));                 // finish after the last descriptor
            ctx.emitter.instruction("ldr x11, [sp, #112]");                   // reload the retained descriptor-vector pointer
            ctx.emitter.instruction("add x10, x9, x9, lsl #1");               // multiply the index by three words
            ctx.emitter.instruction("add x11, x11, x10, lsl #3");             // address the 24-byte descriptor
            ctx.emitter.instruction("ldr w10, [x11]");                         // load the nested ABI value tag
            ctx.emitter.instruction("cmp w10, #8");                            // every eager descriptor must be a bridge handle
            ctx.emitter
                .instruction(&format!("b.ne {}", failure));                   // reject malformed nested values
            ctx.emitter.instruction("ldr w10, [x11, #4]");                     // load the parent/member role flag
            ctx.emitter.instruction("cmp w10, #1");                            // only zero and one are valid roles
            ctx.emitter
                .instruction(&format!("b.hi {}", failure));                   // reject unknown role bits
            ctx.emitter.instruction("str x10, [sp, #152]");                   // preserve the role across wrapper allocation
            ctx.emitter.instruction("ldr x10, [x11, #8]");                     // load the canonical native wrapper handle
            ctx.emitter
                .instruction(&format!("cbz x10, {}", failure));               // canonical handles are always nonzero
            ctx.emitter.instruction("str x10, [sp, #200]");                   // stage the handle across kind dispatch
            ctx.emitter.instruction("ldr x10, [x11, #16]");                    // load the stable concrete wrapper kind
            ctx.emitter
                .instruction(&format!("cbz x10, {}", failure));               // eager members always require a concrete class
            ctx.emitter.instruction("str x10, [sp, #88]");                    // expose the kind to wrapper materialization
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 168]");          // load the next eager descriptor index
            ctx.emitter.instruction("cmp r9, QWORD PTR [rsp + 176]");         // have all wrappers been materialized?
            ctx.emitter
                .instruction(&format!("jae {}", loop_done));                  // finish after the last descriptor
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 112]");        // reload the retained descriptor-vector pointer
            ctx.emitter.instruction("lea r10, [r9 + r9 * 2]");                // multiply the index by three words
            ctx.emitter.instruction("lea r11, [r11 + r10 * 8]");              // address the 24-byte descriptor
            ctx.emitter.instruction("cmp DWORD PTR [r11], 8");                // every eager descriptor must be a bridge handle
            ctx.emitter
                .instruction(&format!("jne {}", failure));                    // reject malformed nested values
            ctx.emitter.instruction("mov r10d, DWORD PTR [r11 + 4]");         // load the parent/member role flag
            ctx.emitter.instruction("cmp r10d, 1");                            // only zero and one are valid roles
            ctx.emitter
                .instruction(&format!("ja {}", failure));                     // reject unknown role bits
            ctx.emitter.instruction("mov QWORD PTR [rsp + 152], r10");        // preserve the role across wrapper allocation
            ctx.emitter.instruction("mov r10, QWORD PTR [r11 + 8]");          // load the canonical native wrapper handle
            ctx.emitter.instruction("test r10, r10");                          // canonical handles are always nonzero
            ctx.emitter
                .instruction(&format!("jz {}", failure));                     // reject a null handle
            ctx.emitter.instruction("mov QWORD PTR [rsp + 200], r10");        // stage the handle across kind dispatch
            ctx.emitter.instruction("mov r10, QWORD PTR [r11 + 16]");         // load the stable concrete wrapper kind
            ctx.emitter.instruction("test r10, r10");                          // eager members always require a concrete class
            ctx.emitter
                .instruction(&format!("jz {}", failure));                     // reject a missing wrapper kind
            ctx.emitter.instruction("mov QWORD PTR [rsp + 88], r10");         // expose the kind to wrapper materialization
        }
    }
    let context_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let handle_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &context_reg, 16);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &handle_reg, OBJECT_FIELDS_OFFSET);
    emit_typed_wrapper_result(
        ctx,
        "DOMNode",
        &context_reg,
        &handle_reg,
        true,
        None,
    )?;
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        ARRAY_VALUE_OFFSET,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #152]");                    // reload the parent/member role
            ctx.emitter
                .instruction(&format!("cbz x9, {}", member));                 // zero denotes a visible nodeset member
            ctx.emitter.instruction("ldr x10, [sp, #144]");                   // inspect an unmatched namespace parent
            ctx.emitter
                .instruction(&format!("cbnz x10, {}", failure));              // parent descriptors must pair one-to-one
            ctx.emitter.instruction("ldr x10, [sp, #184]");                   // load the newly materialized parent wrapper
            ctx.emitter.instruction("str x10, [sp, #144]");                   // retain it until the following namespace wrapper
            ctx.emitter
                .instruction(&format!("b {}", advance));                      // parent wrappers are not visible list members
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 152], 0");          // reload the parent/member role
            ctx.emitter
                .instruction(&format!("je {}", member));                      // zero denotes a visible nodeset member
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 144], 0");          // inspect an unmatched namespace parent
            ctx.emitter
                .instruction(&format!("jne {}", failure));                    // parent descriptors must pair one-to-one
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 184]");        // load the newly materialized parent wrapper
            ctx.emitter.instruction("mov QWORD PTR [rsp + 144], r10");        // retain it until the following namespace wrapper
            ctx.emitter
                .instruction(&format!("jmp {}", advance));                    // parent wrappers are not visible list members
        }
    }

    ctx.emitter.label(&member);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #144]");                    // load an optional namespace parent wrapper
            ctx.emitter
                .instruction(&format!("cbz x9, {}", member_without_parent));  // ordinary nodes have no strong parent owner
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 144], 0");          // load an optional namespace parent wrapper
            ctx.emitter
                .instruction(&format!("je {}", member_without_parent));       // ordinary nodes have no strong parent owner
        }
    }
    let object_reg = abi::int_result_reg(ctx.emitter).to_string();
    let hidden_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &object_reg, ARRAY_VALUE_OFFSET);
    emit_wrapper_hidden_address(ctx, &object_reg, &hidden_reg);
    let parent_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &parent_reg, TEMP_RESULT_LO_OFFSET);
    abi::emit_store_to_address(
        ctx.emitter,
        &parent_reg,
        &hidden_reg,
        crate::internal_extensions::NATIVE_WRAPPER_AUX_OWNER_OFFSET,
    );
    store_stack_immediate(ctx, TEMP_RESULT_LO_OFFSET, 0);
    ctx.emitter.label(&member_without_parent);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #160]");                    // load the eager member-array owner
            ctx.emitter.instruction("ldr x1, [sp, #184]");                    // pass the visible member wrapper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
            ctx.emitter.instruction("str x0, [sp, #160]");                    // preserve a COW replacement array
            ctx.emitter.instruction("ldr x0, [sp, #184]");                    // drop the materializer's temporary wrapper owner
            abi::emit_call_label(ctx.emitter, "__rt_decref_object");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 160]");         // load the eager member-array owner
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 184]");        // pass the visible member wrapper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 160], rax");        // preserve a COW replacement array
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 184]");        // drop the materializer's temporary wrapper owner
            abi::emit_call_label(ctx.emitter, "__rt_decref_object");
        }
    }
    ctx.emitter.label(&advance);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #168]");                    // reload the completed descriptor index
            ctx.emitter.instruction("add x9, x9, #1");                         // advance to the next eager descriptor
            ctx.emitter.instruction("str x9, [sp, #168]");                    // persist the next descriptor index
            ctx.emitter
                .instruction(&format!("b {}", loop_head));                    // materialize the remaining wrappers
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add QWORD PTR [rsp + 168], 1");          // advance to the next eager descriptor
            ctx.emitter
                .instruction(&format!("jmp {}", loop_head));                  // materialize the remaining wrappers
        }
    }
    ctx.emitter.label(&loop_done);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #144]");                    // ensure no namespace parent is left unmatched
            ctx.emitter
                .instruction(&format!("cbnz x9, {}", failure));               // reject a truncated parent/member pair
            ctx.emitter.instruction("str xzr, [sp, #88]");                    // restore the top-level static NodeList kind
            let complete = ctx.next_label("dom_xpath_eager_complete");
            ctx.emitter
                .instruction(&format!("b {}", complete));                     // skip malformed-result containment
            ctx.emitter.label(&failure);
            ctx.emitter.instruction("b __rt_dom_bridge_failure");             // contain malformed eager XPath descriptors
            ctx.emitter.label(&complete);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 144], 0");          // ensure no namespace parent is left unmatched
            ctx.emitter
                .instruction(&format!("jne {}", failure));                    // reject a truncated parent/member pair
            ctx.emitter.instruction("mov QWORD PTR [rsp + 88], 0");          // restore the top-level static NodeList kind
            let complete = ctx.next_label("dom_xpath_eager_complete");
            ctx.emitter
                .instruction(&format!("jmp {}", complete));                   // skip malformed-result containment
            ctx.emitter.label(&failure);
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure");           // contain malformed eager XPath descriptors
            ctx.emitter.label(&complete);
        }
    }
    Ok(())
}

/// Transfers the eager member array into the freshly materialized XPath list wrapper.
fn attach_xpath_nodeset_owner(ctx: &mut FunctionContext<'_>) {
    let object_reg = abi::int_result_reg(ctx.emitter).to_string();
    let hidden_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    let owner_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_store_to_sp(ctx.emitter, &object_reg, ARRAY_VALUE_OFFSET);
    emit_wrapper_hidden_address(ctx, &object_reg, &hidden_reg);
    abi::emit_load_temporary_stack_slot(ctx.emitter, &owner_reg, ARRAY_RESULT_OFFSET);
    abi::emit_store_to_address(
        ctx.emitter,
        &owner_reg,
        &hidden_reg,
        crate::internal_extensions::NATIVE_WRAPPER_AUX_OWNER_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, &object_reg, ARRAY_VALUE_OFFSET);
}

/// Adopts the private eager wrapper returned by public-void `next()` and `rewind()`.
fn materialize_simplexml_iterator_move_result(
    ctx: &mut FunctionContext<'_>,
    opcode: u32,
) -> Result<()> {
    let wrapper = ctx.next_label("simplexml_iterator_move_wrapper");
    let done = ctx.next_label("simplexml_iterator_move_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr w9, [sp, #60]");                       // load the private iterator-move result tag
            ctx.emitter.instruction("cmp w9, #8");                              // did the move eagerly materialize a current wrapper?
            ctx.emitter
                .instruction(&format!("b.eq {}", wrapper));                    // adopt the returned wrapper as the parent's strong owner
            ctx.emitter
                .instruction(&format!("cbz w9, {}", done));                    // an exhausted iterator returns the private null tag
            ctx.emitter.instruction("mov x0, #73");                             // classify an undeclared private iterator result tag
            ctx.emitter.instruction("b __rt_dom_bridge_failure_code");          // contain the result-contract mismatch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 60], 8");             // did the move eagerly materialize a current wrapper?
            ctx.emitter
                .instruction(&format!("je {}", wrapper));                      // adopt the returned wrapper as the parent's strong owner
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 60], 0");             // did the iterator reach its end?
            ctx.emitter
                .instruction(&format!("je {}", done));                         // retain no hidden current wrapper at end
            ctx.emitter.instruction("mov eax, 73");                             // classify an undeclared private iterator result tag
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure_code");        // contain the result-contract mismatch
        }
    }
    ctx.emitter.label(&wrapper);
    let context_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let handle_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &context_reg, 16);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &handle_reg,
        RESULT_FRAME_OFFSET + 32,
    );
    emit_typed_wrapper_result(
        ctx,
        "SimpleXMLElement",
        &context_reg,
        &handle_reg,
        false,
        Some(opcode),
    )?;
    emit_store_simplexml_iterator_current_owner(ctx);
    ctx.emitter.label(&done);
    emit_increment_simplexml_iterator_epoch(ctx);
    Ok(())
}

/// Converts an ABI integer-or-null result into Elephc's inline tagged-scalar registers.
fn materialize_tagged_scalar_result(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let integer = ctx.next_label("dom_result_tagged_scalar_int");
    let null = ctx.next_label("dom_result_tagged_scalar_null");
    let done = ctx.next_label("dom_result_tagged_scalar_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr w9, [sp, #60]");                       // load the native integer-or-null result tag
            ctx.emitter.instruction("cmp w9, #2");                              // does the result carry the ABI integer tag?
            ctx.emitter
                .instruction(&format!("b.eq {}", integer));                     // materialize the integer tagged-scalar member
            ctx.emitter
                .instruction(&format!("cbz w9, {}", null));                     // materialize the canonical tagged null for ABI tag zero
            ctx.emitter.instruction("b __rt_dom_bridge_failure");               // reject a native result outside int|null
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 60], 2");             // does the result carry the ABI integer tag?
            ctx.emitter
                .instruction(&format!("je {}", integer));                       // materialize the integer tagged-scalar member
            ctx.emitter.instruction("cmp DWORD PTR [rsp + 60], 0");             // does the result carry the ABI null tag?
            ctx.emitter
                .instruction(&format!("je {}", null));                          // materialize the canonical tagged null
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure");             // reject a native result outside int|null
        }
    }

    ctx.emitter.label(&integer);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        RESULT_FRAME_OFFSET + 32,
    );
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&null);
    crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);

    ctx.emitter.label(&done);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        TEMP_RESULT_LO_OFFSET,
    );
    abi::emit_store_to_sp(
        ctx.emitter,
        crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
        TEMP_RESULT_HI_OFFSET,
    );
    Ok(())
}

/// Materializes one native indexed range into an owned Mixed Elephc array.
fn materialize_result_array(
    ctx: &mut FunctionContext<'_>,
    element_type: &PhpType,
) -> Result<()> {
    require_result_tag(ctx, 5)?;
    let stored_element_type = element_type.codegen_repr();
    if stored_element_type == PhpType::Str {
        return materialize_result_string_array(ctx);
    }
    let stores_mixed = stored_element_type == PhpType::Mixed;
    let stores_libxml_error = matches!(
        &stored_element_type,
        PhpType::Object(class_name) if class_name == "LibXMLError"
    );
    let stores_namespace_info = matches!(
        &stored_element_type,
        PhpType::Object(class_name) if class_name == "Dom\\NamespaceInfo"
    );
    let stores_simplexml = matches!(
        &stored_element_type,
        PhpType::Object(class_name) if class_name == "SimpleXMLElement"
    );
    if !stores_mixed
        && !stores_libxml_error
        && !stores_namespace_info
        && !stores_simplexml
    {
        return Err(CodegenIrError::unsupported(format!(
            "internal extension array element type {element_type:?}"
        )));
    }
    let capacity_ready = ctx.next_label("dom_result_array_capacity_ready");
    let loop_head = ctx.next_label("dom_result_array_loop");
    let loop_done = ctx.next_label("dom_result_array_done");
    let entry_valid = ctx.next_label("dom_result_array_entry_valid");
    let entry_libxml_error = ctx.next_label("dom_result_array_libxml_error");
    let entry_namespace_info =
        ctx.next_label("dom_result_array_namespace_info");
    let entry_simplexml = ctx.next_label("dom_result_array_simplexml");
    let entry_bytes = ctx.next_label("dom_result_array_bytes");
    let entry_materialized =
        ctx.next_label("dom_result_array_entry_materialized");
    let failure = ctx.next_label("dom_result_array_failure");
    let complete = ctx.next_label("dom_result_array_complete");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #80]");                       // load the native indexed result start offset
            ctx.emitter
                .instruction(&format!("cbnz x9, {}", failure));                 // top-level indexed results must begin at value zero
            ctx.emitter.instruction("ldr x0, [sp, #88]");                       // load the native indexed result count
            ctx.emitter.instruction("ldr x9, [sp, #120]");                      // load the complete retained value count
            ctx.emitter.instruction("cmp x0, x9");                              // does the top-level range fit within the retained value vector?
            ctx.emitter
                .instruction(&format!("b.hi {}", failure));                     // reject a top-level range larger than its retained vector
            ctx.emitter.instruction("cmp x0, #4");                              // enforce the runtime's minimum indexed-array capacity
            ctx.emitter
                .instruction(&format!("b.hs {}", capacity_ready));              // retain a native count of at least four
            ctx.emitter.instruction("mov x0, #4");                              // raise small results to the minimum capacity
            ctx.emitter.label(&capacity_ready);
            ctx.emitter.instruction("mov x1, #8");                              // refcounted result arrays store one heap pointer per slot
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 80], 0");             // must the top-level indexed range begin at value zero?
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject a shifted top-level range
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 88]");           // load the native indexed result count
            ctx.emitter.instruction("cmp rdi, QWORD PTR [rsp + 120]");          // does the top-level range fit within the retained value vector?
            ctx.emitter
                .instruction(&format!("ja {}", failure));                       // reject a range larger than its retained vector
            ctx.emitter.instruction("cmp rdi, 4");                              // enforce the runtime's minimum indexed-array capacity
            ctx.emitter
                .instruction(&format!("jae {}", capacity_ready));               // retain a native count of at least four
            ctx.emitter.instruction("mov rdi, 4");                              // raise small results to the minimum capacity
            ctx.emitter.label(&capacity_ready);
            ctx.emitter.instruction("mov rsi, 8");                              // refcounted result arrays store one heap pointer per slot
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &stored_element_type,
    );
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        ARRAY_RESULT_OFFSET,
    );
    store_stack_immediate(ctx, ARRAY_INDEX_OFFSET, 0);
    stage_result_word(ctx, RESULT_FRAME_OFFSET + 40, ARRAY_COUNT_OFFSET);
    ctx.emitter.label(&loop_head);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", ARRAY_INDEX_OFFSET)); // load the next retained ABI value index
            ctx.emitter
                .instruction(&format!("ldr x10, [sp, #{}]", ARRAY_COUNT_OFFSET)); // load the total retained ABI value count
            ctx.emitter.instruction("cmp x9, x10");                             // have all native values been materialized?
            ctx.emitter
                .instruction(&format!("b.hs {}", loop_done));                   // finish after the final value
            ctx.emitter.instruction("ldr x11, [sp, #112]");                     // load the retained ABI value-vector pointer
            ctx.emitter.instruction("add x10, x9, x9, lsl #1");                 // multiply the index by three words
            ctx.emitter.instruction("add x11, x11, x10, lsl #3");               // address the 24-byte ABI value record
            ctx.emitter.instruction("ldr w10, [x11]");                          // load the nested ABI value tag
            if stores_mixed {
                ctx.emitter.instruction("cmp w10, #4");                         // is this Mixed entry a borrowed byte string?
                ctx.emitter
                    .instruction(&format!("b.eq {}", entry_bytes));             // persist and box the exact nested string
                ctx.emitter.instruction("cmp w10, #8");                         // is this Mixed entry a SimpleXML bridge handle?
                ctx.emitter
                    .instruction(&format!("b.eq {}", entry_simplexml));         // materialize and box its concrete PHP wrapper
            } else if stores_simplexml {
                ctx.emitter.instruction("cmp w10, #8");                         // must every XPath entry be a SimpleXML bridge handle?
                ctx.emitter
                    .instruction(&format!("b.eq {}", entry_simplexml));         // materialize the exact SimpleXML wrapper
                ctx.emitter
                    .instruction(&format!("b {}", failure));                    // reject every non-wrapper XPath member
            }
            ctx.emitter.instruction("cmp w10, #11");                            // is the nested value a PHP value-object descriptor?
            ctx.emitter
                .instruction(&format!("b.ne {}", failure));                     // reject unsupported nested value tags
            ctx.emitter.instruction("ldr w10, [x11, #4]");                      // load the stable nested wrapper type
            ctx.emitter
                .instruction(&format!("str x10, [sp, #{}]", TEMP_RESULT_HI_OFFSET)); // retain the nested value-object type across allocation
            if stores_mixed || stores_libxml_error {
                ctx.emitter.instruction("cmp w10, #1");                         // is this a copied LibXMLError value?
                ctx.emitter
                    .instruction(&format!("b.eq {}", entry_valid));             // materialize the supported LibXMLError schema
            }
            if stores_mixed || stores_namespace_info {
                ctx.emitter.instruction("cmp w10, #2");                         // is this a copied Dom\NamespaceInfo value?
                ctx.emitter
                    .instruction(&format!("b.eq {}", entry_valid));             // materialize the supported namespace-info schema
            }
            ctx.emitter
                .instruction(&format!("b {}", failure));                        // reject an unknown nested wrapper type
            ctx.emitter.label(&entry_valid);
            ctx.emitter.instruction("ldr x10, [x11, #8]");                      // load the LibXMLError field-range start
            ctx.emitter
                .instruction(&format!("str x10, [sp, #{}]", OBJECT_FIELDS_OFFSET)); // stage the nested field-range start
            ctx.emitter.instruction("ldr x10, [x11, #16]");                     // load the LibXMLError field count
            ctx.emitter
                .instruction(&format!("str x10, [sp, #{}]", OBJECT_FIELD_COUNT_OFFSET)); // stage the nested field count
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r9, QWORD PTR [rsp + {}]", ARRAY_INDEX_OFFSET)); // load the next retained ABI value index
            ctx.emitter
                .instruction(&format!("cmp r9, QWORD PTR [rsp + {}]", ARRAY_COUNT_OFFSET)); // have all native values been materialized?
            ctx.emitter
                .instruction(&format!("jae {}", loop_done));                    // finish after the final value
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 112]");          // load the retained ABI value-vector pointer
            ctx.emitter.instruction("lea r10, [r9 + r9 * 2]");                  // multiply the index by three words
            ctx.emitter.instruction("lea r11, [r11 + r10 * 8]");                // address the 24-byte ABI value record
            ctx.emitter.instruction("cmp DWORD PTR [r11], 11");                 // is the nested value a PHP value-object descriptor?
            if stores_mixed {
                ctx.emitter.instruction("cmp DWORD PTR [r11], 4");              // is this Mixed entry a borrowed byte string?
                ctx.emitter
                    .instruction(&format!("je {}", entry_bytes));               // persist and box the exact nested string
                ctx.emitter.instruction("cmp DWORD PTR [r11], 8");              // is this Mixed entry a SimpleXML bridge handle?
                ctx.emitter
                    .instruction(&format!("je {}", entry_simplexml));           // materialize and box its concrete PHP wrapper
                ctx.emitter.instruction("cmp DWORD PTR [r11], 11");             // otherwise require a supported PHP value-object descriptor
            } else if stores_simplexml {
                ctx.emitter.instruction("cmp DWORD PTR [r11], 8");              // must every XPath entry be a SimpleXML bridge handle?
                ctx.emitter
                    .instruction(&format!("je {}", entry_simplexml));           // materialize the exact SimpleXML wrapper
                ctx.emitter
                    .instruction(&format!("jmp {}", failure));                  // reject every non-wrapper XPath member
            }
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject unsupported nested value tags
            ctx.emitter.instruction("mov r10d, DWORD PTR [r11 + 4]");           // load the stable nested wrapper type
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [rsp + {}], r10", TEMP_RESULT_HI_OFFSET)); // retain the nested value-object type across allocation
            if stores_mixed || stores_libxml_error {
                ctx.emitter.instruction("cmp r10d, 1");                         // is this a copied LibXMLError value?
                ctx.emitter
                    .instruction(&format!("je {}", entry_valid));               // materialize the supported LibXMLError schema
            }
            if stores_mixed || stores_namespace_info {
                ctx.emitter.instruction("cmp r10d, 2");                         // is this a copied Dom\NamespaceInfo value?
                ctx.emitter
                    .instruction(&format!("je {}", entry_valid));               // materialize the supported namespace-info schema
            }
            ctx.emitter
                .instruction(&format!("jmp {}", failure));                      // reject an unknown nested wrapper type
            ctx.emitter.label(&entry_valid);
            ctx.emitter.instruction("mov r10, QWORD PTR [r11 + 8]");            // load the LibXMLError field-range start
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [rsp + {}], r10", OBJECT_FIELDS_OFFSET)); // stage the nested field-range start
            ctx.emitter.instruction("mov r10, QWORD PTR [r11 + 16]");           // load the LibXMLError field count
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [rsp + {}], r10", OBJECT_FIELD_COUNT_OFFSET)); // stage the nested field count
        }
    }
    if stores_mixed {
        let subtype_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            &subtype_reg,
            TEMP_RESULT_HI_OFFSET,
        );
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, #1", subtype_reg));   // is this nested value a copied LibXMLError?
                ctx.emitter
                    .instruction(&format!("b.eq {}", entry_libxml_error));      // materialize the LibXMLError ordinary object
                ctx.emitter
                    .instruction(&format!("b {}", entry_namespace_info));       // the validated remaining subtype is Dom\NamespaceInfo
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, 1", subtype_reg));    // is this nested value a copied LibXMLError?
                ctx.emitter
                    .instruction(&format!("je {}", entry_libxml_error));        // materialize the LibXMLError ordinary object
                ctx.emitter
                    .instruction(&format!("jmp {}", entry_namespace_info));     // the validated remaining subtype is Dom\NamespaceInfo
            }
        }
        ctx.emitter.label(&entry_libxml_error);
        materialize_libxml_error_value_object(ctx)?;
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("LibXMLError".to_string()),
        );
        abi::emit_jump(ctx.emitter, &entry_materialized);
        ctx.emitter.label(&entry_namespace_info);
        materialize_namespace_info_value_object(ctx)?;
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Object("Dom\\NamespaceInfo".to_string()),
        );
        abi::emit_jump(ctx.emitter, &entry_materialized);
    } else if stores_libxml_error {
        materialize_libxml_error_value_object(ctx)?;
        abi::emit_jump(ctx.emitter, &entry_materialized);
    } else if stores_namespace_info {
        materialize_namespace_info_value_object(ctx)?;
        abi::emit_jump(ctx.emitter, &entry_materialized);
    } else {
        emit_bridge_failure_jump(ctx);
    }
    if stores_mixed {
        ctx.emitter.label(&entry_bytes);
        materialize_current_array_bytes_entry(ctx, &failure)?;
        emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Str);
        abi::emit_jump(ctx.emitter, &entry_materialized);
    }
    if stores_mixed || stores_simplexml {
        ctx.emitter.label(&entry_simplexml);
        materialize_current_array_simplexml_entry(ctx)?;
        if stores_mixed {
            emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Object("SimpleXMLElement".to_string()),
            );
        }
        abi::emit_jump(ctx.emitter, &entry_materialized);
    }
    ctx.emitter.label(&entry_materialized);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        ARRAY_VALUE_OFFSET,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x0, [sp, #{}]", ARRAY_RESULT_OFFSET)); // load the current Mixed array owner
            ctx.emitter
                .instruction(&format!("ldr x1, [sp, #{}]", ARRAY_VALUE_OFFSET)); // pass the boxed error object to append
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
            abi::emit_store_to_sp(ctx.emitter, "x0", ARRAY_RESULT_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", ARRAY_VALUE_OFFSET);
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", ARRAY_RESULT_OFFSET)); // load the current Mixed array owner
            ctx.emitter
                .instruction(&format!("mov rsi, QWORD PTR [rsp + {}]", ARRAY_VALUE_OFFSET)); // pass the boxed error object to append
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
            abi::emit_store_to_sp(ctx.emitter, "rax", ARRAY_RESULT_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", ARRAY_VALUE_OFFSET);
        }
    }
    abi::emit_decref_if_refcounted(ctx.emitter, &stored_element_type);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", ARRAY_INDEX_OFFSET)); // reload the completed native value index
            ctx.emitter.instruction("add x9, x9, #1");                          // advance to the next retained ABI value
            ctx.emitter
                .instruction(&format!("str x9, [sp, #{}]", ARRAY_INDEX_OFFSET)); // persist the next loop index
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("add QWORD PTR [rsp + {}], 1", ARRAY_INDEX_OFFSET)); // advance to the next retained ABI value
        }
    }
    abi::emit_jump(ctx.emitter, &loop_head);
    ctx.emitter.label(&loop_done);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        ARRAY_RESULT_OFFSET,
    );
    abi::emit_jump(ctx.emitter, &complete);
    ctx.emitter.label(&failure);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("b __rt_dom_bridge_failure"),  // contain a malformed nested native result
        Arch::X86_64 => ctx.emitter.instruction("jmp __rt_dom_bridge_failure"), // contain a malformed nested native result
    }
    ctx.emitter.label(&complete);
    Ok(())
}

/// Persists the byte-string record at the current native array index.
fn materialize_current_array_bytes_entry(
    ctx: &mut FunctionContext<'_>,
    failure: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", ARRAY_INDEX_OFFSET)); // reload the current flat value index
            ctx.emitter.instruction("ldr x11, [sp, #112]");                     // reload the retained ABI value-vector pointer
            ctx.emitter.instruction("add x10, x9, x9, lsl #1");                 // multiply the index by three words
            ctx.emitter.instruction("add x11, x11, x10, lsl #3");               // address the current 24-byte ABI value
            ctx.emitter.instruction("ldr x10, [x11, #8]");                      // load the borrowed byte offset
            ctx.emitter.instruction("ldr x2, [x11, #16]");                      // load the exact byte length
            ctx.emitter.instruction("adds x9, x10, x2");                        // compute the exclusive byte-range end
            ctx.emitter
                .instruction(&format!("b.cs {}", failure));                    // reject an overflowing nested byte range
            ctx.emitter.instruction("ldr x11, [sp, #104]");                     // load the retained byte-buffer length
            ctx.emitter.instruction("cmp x9, x11");                             // does the nested string fit the result bytes?
            ctx.emitter
                .instruction(&format!("b.hi {}", failure));                    // reject an out-of-range nested string
            ctx.emitter.instruction("ldr x1, [sp, #96]");                       // load the retained byte-buffer base
            ctx.emitter.instruction("add x1, x1, x10");                         // address the borrowed nested string
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r9, QWORD PTR [rsp + {}]", ARRAY_INDEX_OFFSET)); // reload the current flat value index
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 112]");          // reload the retained ABI value-vector pointer
            ctx.emitter.instruction("lea r10, [r9 + r9 * 2]");                  // multiply the index by three words
            ctx.emitter.instruction("lea r11, [r11 + r10 * 8]");                // address the current 24-byte ABI value
            ctx.emitter.instruction("mov r10, QWORD PTR [r11 + 8]");            // load the borrowed byte offset
            ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 16]");           // load the exact byte length
            ctx.emitter.instruction("mov r9, r10");                             // preserve the offset for pointer materialization
            ctx.emitter.instruction("add r9, rdx");                             // compute the exclusive byte-range end
            ctx.emitter
                .instruction(&format!("jc {}", failure));                      // reject an overflowing nested byte range
            ctx.emitter.instruction("cmp r9, QWORD PTR [rsp + 104]");           // does the nested string fit the result bytes?
            ctx.emitter
                .instruction(&format!("ja {}", failure));                      // reject an out-of-range nested string
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 96]");           // load the retained byte-buffer base
            ctx.emitter.instruction("add rax, r10");                            // address the borrowed nested string
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    Ok(())
}

/// Materializes the SimpleXML handle record at the current native array index.
fn materialize_current_array_simplexml_entry(
    ctx: &mut FunctionContext<'_>,
) -> Result<()> {
    let record_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let index_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &index_reg,
        ARRAY_INDEX_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, &record_reg, 112);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(
                "add {}, {}, {}, lsl #1",
                index_reg, index_reg, index_reg
            )); // multiply the current value index by three words
            ctx.emitter.instruction(&format!(
                "add {}, {}, {}, lsl #3",
                record_reg, record_reg, index_reg
            )); // address the current 24-byte bridge-handle record
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!(
                "lea {}, [{} + {} * 2]",
                index_reg, index_reg, index_reg
            )); // multiply the current value index by three words
            ctx.emitter.instruction(&format!(
                "lea {}, [{} + {} * 8]",
                record_reg, record_reg, index_reg
            )); // address the current 24-byte bridge-handle record
        }
    }
    abi::emit_load_from_address(ctx.emitter, &index_reg, &record_reg, 8);
    abi::emit_store_to_sp(ctx.emitter, &index_reg, OBJECT_FIELDS_OFFSET);
    abi::emit_load_from_address(ctx.emitter, &index_reg, &record_reg, 16);
    abi::emit_store_to_sp(
        ctx.emitter,
        &index_reg,
        RESULT_FRAME_OFFSET + 40,
    );
    let context_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let handle_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &context_reg, 16);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &handle_reg,
        OBJECT_FIELDS_OFFSET,
    );
    emit_typed_wrapper_result(
        ctx,
        "SimpleXMLElement",
        &context_reg,
        &handle_reg,
        false,
        None,
    )
}

/// Materializes one native indexed byte-string range into an owned string array.
fn materialize_result_string_array(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let capacity_ready = ctx.next_label("dom_string_array_capacity_ready");
    let loop_head = ctx.next_label("dom_string_array_loop");
    let loop_done = ctx.next_label("dom_string_array_done");
    let failure = ctx.next_label("dom_string_array_failure");
    let complete = ctx.next_label("dom_string_array_complete");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #80]");                       // load the native indexed result start offset
            ctx.emitter
                .instruction(&format!("cbnz x9, {}", failure));                 // top-level string arrays must begin at value zero
            ctx.emitter.instruction("ldr x0, [sp, #88]");                       // load the native string-array item count
            ctx.emitter.instruction("ldr x9, [sp, #120]");                      // load the complete retained value count
            ctx.emitter.instruction("cmp x0, x9");                              // does the declared array fit the retained value vector?
            ctx.emitter
                .instruction(&format!("b.hi {}", failure));                     // reject an oversized top-level range
            ctx.emitter.instruction("cmp x0, #4");                              // enforce the runtime's minimum indexed-array capacity
            ctx.emitter
                .instruction(&format!("b.hs {}", capacity_ready));              // retain a native count of at least four
            ctx.emitter.instruction("mov x0, #4");                              // raise small results to the minimum capacity
            ctx.emitter.label(&capacity_ready);
            ctx.emitter.instruction("mov x1, #16");                             // string arrays store one pointer and one byte length per slot
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 80], 0");             // must the top-level string range begin at value zero?
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject a shifted top-level string range
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 88]");           // load the native string-array item count
            ctx.emitter.instruction("cmp rdi, QWORD PTR [rsp + 120]");          // does the declared array fit the retained value vector?
            ctx.emitter
                .instruction(&format!("ja {}", failure));                       // reject an oversized top-level range
            ctx.emitter.instruction("cmp rdi, 4");                              // enforce the runtime's minimum indexed-array capacity
            ctx.emitter
                .instruction(&format!("jae {}", capacity_ready));               // retain a native count of at least four
            ctx.emitter.instruction("mov rdi, 4");                              // raise small results to the minimum capacity
            ctx.emitter.label(&capacity_ready);
            ctx.emitter.instruction("mov rsi, 16");                             // string arrays store one pointer and one byte length per slot
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &PhpType::Str,
    );
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        ARRAY_RESULT_OFFSET,
    );
    store_stack_immediate(ctx, ARRAY_INDEX_OFFSET, 0);
    stage_result_word(ctx, RESULT_FRAME_OFFSET + 40, ARRAY_COUNT_OFFSET);
    ctx.emitter.label(&loop_head);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", ARRAY_INDEX_OFFSET)); // load the next retained ABI string index
            ctx.emitter
                .instruction(&format!("ldr x10, [sp, #{}]", ARRAY_COUNT_OFFSET)); // load the declared string count
            ctx.emitter.instruction("cmp x9, x10");                             // have all native strings been materialized?
            ctx.emitter
                .instruction(&format!("b.hs {}", loop_done));                   // finish after the final string
            ctx.emitter.instruction("ldr x11, [sp, #112]");                     // load the retained ABI value-vector pointer
            ctx.emitter.instruction("add x10, x9, x9, lsl #1");                 // multiply the index by three words
            ctx.emitter.instruction("add x11, x11, x10, lsl #3");               // address the 24-byte ABI string record
            ctx.emitter.instruction("ldr w10, [x11]");                          // load the nested ABI value tag
            ctx.emitter.instruction("cmp w10, #4");                             // must this nested value be a byte string?
            ctx.emitter
                .instruction(&format!("b.ne {}", failure));                     // reject a non-string array member
            ctx.emitter.instruction("ldr x10, [x11, #8]");                      // load the string's retained byte offset
            ctx.emitter.instruction("ldr x2, [x11, #16]");                      // load the string's retained byte length
            ctx.emitter.instruction("adds x9, x10, x2");                        // compute the exclusive byte-range end
            ctx.emitter
                .instruction(&format!("b.cs {}", failure));                     // reject an overflowing byte range
            ctx.emitter.instruction("ldr x11, [sp, #104]");                     // load the retained byte-buffer length
            ctx.emitter.instruction("cmp x9, x11");                             // does the nested string fit the retained bytes?
            ctx.emitter
                .instruction(&format!("b.hi {}", failure));                     // reject an out-of-range nested string
            ctx.emitter.instruction("ldr x1, [sp, #96]");                       // load the retained byte-buffer base
            ctx.emitter.instruction("add x1, x1, x10");                         // address the borrowed nested string
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r9, QWORD PTR [rsp + {}]", ARRAY_INDEX_OFFSET)); // load the next retained ABI string index
            ctx.emitter
                .instruction(&format!("cmp r9, QWORD PTR [rsp + {}]", ARRAY_COUNT_OFFSET)); // have all native strings been materialized?
            ctx.emitter
                .instruction(&format!("jae {}", loop_done));                    // finish after the final string
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 112]");          // load the retained ABI value-vector pointer
            ctx.emitter.instruction("lea r10, [r9 + r9 * 2]");                  // multiply the index by three words
            ctx.emitter.instruction("lea r11, [r11 + r10 * 8]");                // address the 24-byte ABI string record
            ctx.emitter.instruction("cmp DWORD PTR [r11], 4");                  // must this nested value be a byte string?
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject a non-string array member
            ctx.emitter.instruction("mov r10, QWORD PTR [r11 + 8]");            // load the string's retained byte offset
            ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 16]");           // load the string's retained byte length
            ctx.emitter.instruction("mov r9, r10");                             // preserve the byte offset for pointer materialization
            ctx.emitter.instruction("add r9, rdx");                             // compute the exclusive byte-range end
            ctx.emitter
                .instruction(&format!("jc {}", failure));                       // reject an overflowing byte range
            ctx.emitter.instruction("cmp r9, QWORD PTR [rsp + 104]");           // does the nested string fit the retained bytes?
            ctx.emitter
                .instruction(&format!("ja {}", failure));                       // reject an out-of-range nested string
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 96]");           // load the retained byte-buffer base
            ctx.emitter.instruction("add rax, r10");                            // address the borrowed nested string
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    let (pointer_reg, length_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_store_to_sp(
        ctx.emitter,
        pointer_reg,
        ARRAY_VALUE_OFFSET,
    );
    abi::emit_store_to_sp(
        ctx.emitter,
        length_reg,
        OBJECT_RESULT_OFFSET,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "x0",
                ARRAY_RESULT_OFFSET,
            );
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "x1",
                ARRAY_VALUE_OFFSET,
            );
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "x2",
                OBJECT_RESULT_OFFSET,
            );
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
            abi::emit_store_to_sp(ctx.emitter, "x0", ARRAY_RESULT_OFFSET);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "rdi",
                ARRAY_RESULT_OFFSET,
            );
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "rsi",
                ARRAY_VALUE_OFFSET,
            );
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "rdx",
                OBJECT_RESULT_OFFSET,
            );
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
            abi::emit_store_to_sp(ctx.emitter, "rax", ARRAY_RESULT_OFFSET);
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", ARRAY_INDEX_OFFSET)); // reload the completed native string index
            ctx.emitter.instruction("add x9, x9, #1");                          // advance to the next retained string
            ctx.emitter
                .instruction(&format!("str x9, [sp, #{}]", ARRAY_INDEX_OFFSET)); // persist the next string index
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("add QWORD PTR [rsp + {}], 1", ARRAY_INDEX_OFFSET)); // advance to the next retained string
        }
    }
    abi::emit_jump(ctx.emitter, &loop_head);
    ctx.emitter.label(&loop_done);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        ARRAY_RESULT_OFFSET,
    );
    abi::emit_jump(ctx.emitter, &complete);
    ctx.emitter.label(&failure);
    emit_bridge_failure_jump(ctx);
    ctx.emitter.label(&complete);
    Ok(())
}

/// Materializes one native alternating key/value range into an owned associative array.
fn materialize_result_map(
    ctx: &mut FunctionContext<'_>,
    key_type: &PhpType,
    value_type: &PhpType,
) -> Result<()> {
    require_result_tag(ctx, 6)?;
    let stored_key_type = key_type.codegen_repr();
    let stored_value_type = value_type.codegen_repr();
    if stored_key_type != PhpType::Str && stored_key_type != PhpType::Mixed {
        return Err(CodegenIrError::unsupported(format!(
            "internal extension map key type {key_type:?}"
        )));
    }
    let stores_mixed = stored_value_type == PhpType::Mixed;
    if stored_value_type != PhpType::Str && !stores_mixed {
        return Err(CodegenIrError::unsupported(format!(
            "internal extension map value type {value_type:?}"
        )));
    }
    if stores_mixed {
        return result_tree::materialize_recursive_result_map(
            ctx,
            stored_key_type == PhpType::Mixed,
        );
    }
    let capacity_ready = ctx.next_label("dom_result_map_capacity_ready");
    let loop_head = ctx.next_label("dom_result_map_loop");
    let loop_done = ctx.next_label("dom_result_map_done");
    let failure = ctx.next_label("dom_result_map_failure");
    let complete = ctx.next_label("dom_result_map_complete");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #80]");                       // load the native map range start offset
            ctx.emitter
                .instruction(&format!("cbnz x9, {}", failure));                 // top-level maps must begin at flat value zero
            ctx.emitter.instruction("ldr x0, [sp, #88]");                       // load the native map entry count
            ctx.emitter.instruction("adds x9, x0, x0");                         // compute the required alternating record count
            ctx.emitter
                .instruction(&format!("b.cs {}", failure));                     // reject an overflowing pair count
            ctx.emitter.instruction("ldr x10, [sp, #120]");                     // load the retained flat value count
            ctx.emitter.instruction("cmp x9, x10");                             // does the vector contain exactly two records per entry?
            ctx.emitter
                .instruction(&format!("b.ne {}", failure));                     // reject a truncated or trailing map range
            ctx.emitter.instruction("lsl x0, x0, #1");                          // target a load factor below fifty percent
            ctx.emitter.instruction("cmp x0, #16");                             // enforce the runtime's minimum hash capacity
            ctx.emitter
                .instruction(&format!("b.hs {}", capacity_ready));              // retain a sufficiently large computed capacity
            ctx.emitter.instruction("mov x0, #16");                             // raise small maps to the minimum capacity
            ctx.emitter.label(&capacity_ready);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x1",
                crate::codegen::runtime_value_tag(&stored_value_type) as i64,
            );
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rsp + 80], 0");             // must the top-level map begin at flat value zero?
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject a shifted top-level map range
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 88]");           // load the native map entry count
            ctx.emitter.instruction("mov r10, rdi");                            // preserve the entry count for range validation
            ctx.emitter.instruction("shl r10, 1");                              // compute the required alternating record count
            ctx.emitter
                .instruction(&format!("jc {}", failure));                       // reject an overflowing pair count
            ctx.emitter.instruction("cmp r10, QWORD PTR [rsp + 120]");          // does the vector contain exactly two records per entry?
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject a truncated or trailing map range
            ctx.emitter.instruction("shl rdi, 1");                              // target a load factor below fifty percent
            ctx.emitter.instruction("cmp rdi, 16");                             // enforce the runtime's minimum hash capacity
            ctx.emitter
                .instruction(&format!("jae {}", capacity_ready));               // retain a sufficiently large computed capacity
            ctx.emitter.instruction("mov rdi, 16");                             // raise small maps to the minimum capacity
            ctx.emitter.label(&capacity_ready);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "rsi",
                crate::codegen::runtime_value_tag(&stored_value_type) as i64,
            );
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        ARRAY_RESULT_OFFSET,
    );
    store_stack_immediate(ctx, ARRAY_INDEX_OFFSET, 0);
    stage_result_word(ctx, RESULT_FRAME_OFFSET + 40, ARRAY_COUNT_OFFSET);
    ctx.emitter.label(&loop_head);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", ARRAY_INDEX_OFFSET)); // load the next native map entry index
            ctx.emitter
                .instruction(&format!("ldr x10, [sp, #{}]", ARRAY_COUNT_OFFSET)); // load the total native map entry count
            ctx.emitter.instruction("cmp x9, x10");                             // have all key/value pairs been materialized?
            ctx.emitter
                .instruction(&format!("b.hs {}", loop_done));                   // finish after the final pair
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r9, QWORD PTR [rsp + {}]", ARRAY_INDEX_OFFSET)); // load the next native map entry index
            ctx.emitter
                .instruction(&format!("cmp r9, QWORD PTR [rsp + {}]", ARRAY_COUNT_OFFSET)); // have all key/value pairs been materialized?
            ctx.emitter
                .instruction(&format!("jae {}", loop_done));                    // finish after the final pair
        }
    }
    materialize_current_map_bytes_entry(ctx, 0, &failure)?;
    let (pointer_reg, length_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, pointer_reg, OBJECT_FIELDS_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, length_reg, OBJECT_FIELD_COUNT_OFFSET);
    materialize_current_map_bytes_entry(ctx, 1, &failure)?;
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    if stores_mixed {
        emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Str);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if stores_mixed {
                ctx.emitter.instruction("mov x3, x0");                          // pass the owned boxed Mixed value to hash insertion
                ctx.emitter.instruction("mov x4, xzr");                         // boxed Mixed values use one payload word
            } else {
                ctx.emitter.instruction("mov x3, x1");                          // transfer the owned string pointer to the hash entry
                ctx.emitter.instruction("mov x4, x2");                          // transfer the owned string length to the hash entry
            }
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", ARRAY_RESULT_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", OBJECT_FIELDS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", OBJECT_FIELD_COUNT_OFFSET);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "x5",
                crate::codegen::runtime_value_tag(&stored_value_type) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            abi::emit_store_to_sp(ctx.emitter, "x0", ARRAY_RESULT_OFFSET);
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", ARRAY_INDEX_OFFSET)); // reload the completed native map entry index
            ctx.emitter.instruction("add x9, x9, #1");                          // advance to the next key/value pair
            ctx.emitter
                .instruction(&format!("str x9, [sp, #{}]", ARRAY_INDEX_OFFSET)); // persist the next map entry index
        }
        Arch::X86_64 => {
            if stores_mixed {
                ctx.emitter.instruction("mov rcx, rax");                        // pass the owned boxed Mixed value to hash insertion
                ctx.emitter.instruction("xor r8, r8");                          // boxed Mixed values use one payload word
            } else {
                ctx.emitter.instruction("mov rcx, rax");                        // transfer the owned string pointer to the hash entry
                ctx.emitter.instruction("mov r8, rdx");                         // transfer the owned string length to the hash entry
            }
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", ARRAY_RESULT_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", OBJECT_FIELDS_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", OBJECT_FIELD_COUNT_OFFSET);
            abi::emit_load_int_immediate(
                ctx.emitter,
                "r9",
                crate::codegen::runtime_value_tag(&stored_value_type) as i64,
            );
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            abi::emit_store_to_sp(ctx.emitter, "rax", ARRAY_RESULT_OFFSET);
            ctx.emitter
                .instruction(&format!("add QWORD PTR [rsp + {}], 1", ARRAY_INDEX_OFFSET)); // advance to the next key/value pair
        }
    }
    abi::emit_jump(ctx.emitter, &loop_head);
    ctx.emitter.label(&loop_done);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        ARRAY_RESULT_OFFSET,
    );
    abi::emit_jump(ctx.emitter, &complete);
    ctx.emitter.label(&failure);
    emit_bridge_failure_jump(ctx);
    ctx.emitter.label(&complete);
    Ok(())
}

/// Addresses and validates one string key or value in the current native map pair.
fn materialize_current_map_bytes_entry(
    ctx: &mut FunctionContext<'_>,
    pair_offset: usize,
    failure: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", ARRAY_INDEX_OFFSET)); // reload the current map entry index
            ctx.emitter.instruction("lsl x9, x9, #1");                          // map the pair index to its flat key record
            if pair_offset != 0 {
                ctx.emitter.instruction(&format!("add x9, x9, #{}", pair_offset)); // select the value record after its key
            }
            ctx.emitter.instruction("ldr x11, [sp, #112]");                     // reload the retained ABI value-vector pointer
            ctx.emitter.instruction("add x10, x9, x9, lsl #1");                 // multiply the flat record index by three words
            ctx.emitter.instruction("add x11, x11, x10, lsl #3");               // address the selected 24-byte ABI value
            ctx.emitter.instruction("ldr w10, [x11]");                          // load the selected map record tag
            ctx.emitter.instruction("cmp w10, #4");                             // namespace maps require exact byte-string keys and values
            ctx.emitter
                .instruction(&format!("b.ne {}", failure));                    // reject a non-string map component
            ctx.emitter.instruction("ldr x10, [x11, #8]");                      // load the borrowed byte offset
            ctx.emitter.instruction("ldr x2, [x11, #16]");                      // load the exact byte length
            ctx.emitter.instruction("adds x9, x10, x2");                        // compute the exclusive byte-range end
            ctx.emitter
                .instruction(&format!("b.cs {}", failure));                    // reject an overflowing nested byte range
            ctx.emitter.instruction("ldr x11, [sp, #104]");                     // load the retained byte-buffer length
            ctx.emitter.instruction("cmp x9, x11");                             // does the selected string fit the result bytes?
            ctx.emitter
                .instruction(&format!("b.hi {}", failure));                    // reject an out-of-range map component
            ctx.emitter.instruction("ldr x1, [sp, #96]");                       // load the retained byte-buffer base
            ctx.emitter.instruction("add x1, x1, x10");                         // address the borrowed map string
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r9, QWORD PTR [rsp + {}]", ARRAY_INDEX_OFFSET)); // reload the current map entry index
            ctx.emitter.instruction("shl r9, 1");                               // map the pair index to its flat key record
            if pair_offset != 0 {
                ctx.emitter.instruction(&format!("add r9, {}", pair_offset));   // select the value record after its key
            }
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 112]");          // reload the retained ABI value-vector pointer
            ctx.emitter.instruction("lea r10, [r9 + r9 * 2]");                  // multiply the flat record index by three words
            ctx.emitter.instruction("lea r11, [r11 + r10 * 8]");                // address the selected 24-byte ABI value
            ctx.emitter.instruction("cmp DWORD PTR [r11], 4");                  // namespace maps require exact byte-string keys and values
            ctx.emitter
                .instruction(&format!("jne {}", failure));                     // reject a non-string map component
            ctx.emitter.instruction("mov r10, QWORD PTR [r11 + 8]");            // load the borrowed byte offset
            ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 16]");           // load the exact byte length
            ctx.emitter.instruction("mov r9, r10");                             // preserve the offset for pointer materialization
            ctx.emitter.instruction("add r9, rdx");                             // compute the exclusive byte-range end
            ctx.emitter
                .instruction(&format!("jc {}", failure));                      // reject an overflowing nested byte range
            ctx.emitter.instruction("cmp r9, QWORD PTR [rsp + 104]");           // does the selected string fit the result bytes?
            ctx.emitter
                .instruction(&format!("ja {}", failure));                      // reject an out-of-range map component
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 96]");           // load the retained byte-buffer base
            ctx.emitter.instruction("add rax, r10");                            // address the borrowed map string
        }
    }
    Ok(())
}

/// Materializes three flat fields into an ordinary readonly `Dom\NamespaceInfo`.
fn materialize_namespace_info_value_object(
    ctx: &mut FunctionContext<'_>,
) -> Result<()> {
    let offsets = namespace_info_property_offsets(ctx)?;
    let failure = ctx.next_label("dom_namespace_info_value_failure");
    let complete = ctx.next_label("dom_namespace_info_value_complete");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", OBJECT_FIELD_COUNT_OFFSET)); // load the declared namespace-info field count
            ctx.emitter.instruction("cmp x9, #3");                              // does this descriptor contain exactly three fields?
            ctx.emitter
                .instruction(&format!("b.ne {}", failure));                     // reject an incompatible value-object schema
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", OBJECT_FIELDS_OFFSET)); // load the flat field-range start
            ctx.emitter.instruction("adds x10, x9, #3");                        // compute the exclusive field-range end with overflow detection
            ctx.emitter
                .instruction(&format!("b.cs {}", failure));                     // reject an overflowing field range
            ctx.emitter.instruction("ldr x11, [sp, #120]");                     // load the retained flat value count
            ctx.emitter.instruction("cmp x10, x11");                            // does the complete field range fit the retained result?
            ctx.emitter
                .instruction(&format!("b.hi {}", failure));                     // reject a field range outside native result storage
            ctx.emitter.instruction("ldr x11, [sp, #112]");                     // load the retained flat value-vector pointer
            ctx.emitter.instruction("add x10, x9, x9, lsl #1");                 // multiply the start index by three ABI words
            ctx.emitter.instruction("add x11, x11, x10, lsl #3");               // address the first 24-byte field record
            ctx.emitter
                .instruction(&format!("str x11, [sp, #{}]", OBJECT_FIELDS_OFFSET)); // retain the first field pointer across allocations
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp QWORD PTR [rsp + {}], 3", OBJECT_FIELD_COUNT_OFFSET)); // must this descriptor expose exactly three fields?
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject an incompatible value-object schema
            ctx.emitter
                .instruction(&format!("mov r10, QWORD PTR [rsp + {}]", OBJECT_FIELDS_OFFSET)); // load the flat field-range start
            ctx.emitter.instruction("mov r11, r10");                            // preserve the start for pointer materialization
            ctx.emitter.instruction("add r11, 3");                              // compute the exclusive field-range end
            ctx.emitter
                .instruction(&format!("jc {}", failure));                       // reject an overflowing field range
            ctx.emitter.instruction("cmp r11, QWORD PTR [rsp + 120]");          // does the complete field range fit the retained result?
            ctx.emitter
                .instruction(&format!("ja {}", failure));                       // reject a field range outside native result storage
            ctx.emitter.instruction("lea r10, [r10 + r10 * 2]");                // multiply the start index by three ABI words
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 112]");          // load the retained flat value-vector pointer
            ctx.emitter.instruction("lea r11, [r11 + r10 * 8]");                // address the first 24-byte field record
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [rsp + {}], r11", OBJECT_FIELDS_OFFSET)); // retain the first field pointer across allocations
        }
    }

    super::objects::emit_internal_value_object_allocation(
        ctx,
        "Dom\\NamespaceInfo",
    )?;
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        OBJECT_RESULT_OFFSET,
    );
    emit_namespace_info_nullable_string_field(ctx, 0, offsets[0], &failure);
    emit_namespace_info_nullable_string_field(ctx, 1, offsets[1], &failure);
    emit_namespace_info_element_field(ctx, 2, offsets[2], &failure)?;
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        OBJECT_RESULT_OFFSET,
    );
    abi::emit_jump(ctx.emitter, &complete);
    ctx.emitter.label(&failure);
    emit_bridge_failure_jump(ctx);
    ctx.emitter.label(&complete);
    Ok(())
}

/// Resolves and validates the physical property layout of `Dom\NamespaceInfo`.
fn namespace_info_property_offsets(
    ctx: &FunctionContext<'_>,
) -> Result<[usize; 3]> {
    let class_name = "Dom\\NamespaceInfo";
    let class_info = ctx
        .module
        .class_infos
        .get(class_name)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {class_name}")))?;
    let properties = [
        ("prefix", PhpType::Mixed),
        ("namespaceURI", PhpType::Mixed),
        ("element", PhpType::Object("Dom\\Element".to_string())),
    ];
    let mut offsets = [0; 3];
    for (index, (property, expected_type)) in properties.into_iter().enumerate() {
        let (_, (_, actual_type)) = class_info.visible_property(property).ok_or_else(|| {
            CodegenIrError::invalid_module(format!(
                "{class_name} is missing declared property ${property}"
            ))
        })?;
        if actual_type.codegen_repr() != expected_type {
            return Err(CodegenIrError::invalid_module(format!(
                "{class_name}::${property} has incompatible type {actual_type:?}"
            )));
        }
        offsets[index] = *class_info.property_offsets.get(property).ok_or_else(|| {
            CodegenIrError::invalid_module(format!(
                "{class_name}::${property} has no object-layout offset"
            ))
        })?;
    }
    Ok(offsets)
}

/// Persists one nullable namespace string into its boxed readonly property slot.
fn emit_namespace_info_nullable_string_field(
    ctx: &mut FunctionContext<'_>,
    field_index: usize,
    property_offset: usize,
    failure: &str,
) {
    let field_offset = field_index * ABI_VALUE_SIZE;
    let null = ctx.next_label("dom_namespace_info_string_null");
    let ready = ctx.next_label("dom_namespace_info_string_ready");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", OBJECT_FIELDS_OFFSET)); // reload the first namespace-info field record
            ctx.emitter
                .instruction(&format!("ldr w10, [x9, #{}]", field_offset));     // load this nullable string's ABI value tag
            ctx.emitter.instruction("cmp w10, #0");                             // is this namespace component null?
            ctx.emitter
                .instruction(&format!("b.eq {}", null));                        // materialize PHP null for an absent namespace component
            ctx.emitter.instruction("cmp w10, #4");                             // must every non-null namespace component be bytes?
            ctx.emitter
                .instruction(&format!("b.ne {}", failure));                     // reject a mismatched namespace string tag
            ctx.emitter
                .instruction(&format!("ldr x10, [x9, #{}]", field_offset + 8)); // load the byte-range offset
            ctx.emitter
                .instruction(&format!("ldr x11, [x9, #{}]", field_offset + 16)); // load the byte-range length
            ctx.emitter.instruction("ldr x1, [sp, #96]");                       // load the retained native byte-buffer pointer
            ctx.emitter.instruction("add x1, x1, x10");                         // address this exact namespace string
            ctx.emitter.instruction("adds x10, x10, x11");                      // compute the exclusive byte-range end with overflow detection
            ctx.emitter
                .instruction(&format!("b.cs {}", failure));                     // reject an overflowing byte range
            ctx.emitter.instruction("ldr x9, [sp, #104]");                      // load the retained native byte-buffer length
            ctx.emitter.instruction("cmp x10, x9");                             // does this byte range fit the retained result?
            ctx.emitter
                .instruction(&format!("b.hi {}", failure));                     // reject bytes outside native result storage
            ctx.emitter.instruction("mov x2, x11");                             // pass the exact byte length to string persistence
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r10, QWORD PTR [rsp + {}]", OBJECT_FIELDS_OFFSET)); // reload the first namespace-info field record
            ctx.emitter
                .instruction(&format!("cmp DWORD PTR [r10 + {}], 0", field_offset)); // is this namespace component null?
            ctx.emitter
                .instruction(&format!("je {}", null));                          // materialize PHP null for an absent namespace component
            ctx.emitter
                .instruction(&format!("cmp DWORD PTR [r10 + {}], 4", field_offset)); // must every non-null namespace component be bytes?
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject a mismatched namespace string tag
            ctx.emitter
                .instruction(&format!("mov r11, QWORD PTR [r10 + {}]", field_offset + 8)); // load the byte-range offset
            ctx.emitter
                .instruction(&format!("mov rdx, QWORD PTR [r10 + {}]", field_offset + 16)); // load the byte-range length
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 96]");           // load the retained native byte-buffer pointer
            ctx.emitter.instruction("add rax, r11");                            // address this exact namespace string
            ctx.emitter.instruction("add r11, rdx");                            // compute the exclusive byte-range end
            ctx.emitter
                .instruction(&format!("jc {}", failure));                       // reject an overflowing byte range
            ctx.emitter.instruction("cmp r11, QWORD PTR [rsp + 104]");          // does this byte range fit the retained result?
            ctx.emitter
                .instruction(&format!("ja {}", failure));                       // reject bytes outside native result storage
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Str);
    abi::emit_jump(ctx.emitter, &ready);
    ctx.emitter.label(&null);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        crate::codegen::NULL_SENTINEL,
    );
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
    ctx.emitter.label(&ready);
    let object_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &object_reg,
        OBJECT_RESULT_OFFSET,
    );
    abi::emit_store_to_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &object_reg,
        property_offset,
    );
    abi::emit_store_zero_to_address(
        ctx.emitter,
        &object_reg,
        property_offset + 8,
    );
}

/// Materializes one canonical modern element wrapper into the readonly element field.
fn emit_namespace_info_element_field(
    ctx: &mut FunctionContext<'_>,
    field_index: usize,
    property_offset: usize,
    failure: &str,
) -> Result<()> {
    let field_offset = field_index * ABI_VALUE_SIZE;
    let kind_valid = ctx.next_label("dom_namespace_info_element_kind_valid");
    let field_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let handle_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    let kind_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &field_reg,
        OBJECT_FIELDS_OFFSET,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr w9, [{}, #{}]", field_reg, field_offset)); // load the element field's ABI value tag without clobbering its descriptor pointer
            ctx.emitter.instruction("cmp w9, #8");                              // must the namespace owner be a bridge handle?
            ctx.emitter
                .instruction(&format!("b.ne {}", failure));                     // reject a non-wrapper namespace owner
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp DWORD PTR [{} + {}], 8", field_reg, field_offset)); // must the namespace owner be a bridge handle?
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject a non-wrapper namespace owner
        }
    }
    abi::emit_load_from_address(
        ctx.emitter,
        &handle_reg,
        &field_reg,
        field_offset + 8,
    );
    abi::emit_load_from_address(
        ctx.emitter,
        &kind_reg,
        &field_reg,
        field_offset + 16,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #201", kind_reg));        // is this a generic modern XML element?
            ctx.emitter
                .instruction(&format!("b.eq {}", kind_valid));                  // accept the generic Dom\Element wrapper kind
            ctx.emitter.instruction(&format!("cmp {}, #301", kind_reg));        // is this a modern HTML element?
            ctx.emitter
                .instruction(&format!("b.ne {}", failure));                     // reject every non-element wrapper kind
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 201", kind_reg));         // is this a generic modern XML element?
            ctx.emitter
                .instruction(&format!("je {}", kind_valid));                    // accept the generic Dom\Element wrapper kind
            ctx.emitter.instruction(&format!("cmp {}, 301", kind_reg));         // is this a modern HTML element?
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject every non-element wrapper kind
        }
    }
    ctx.emitter.label(&kind_valid);
    abi::emit_store_to_sp(
        ctx.emitter,
        &kind_reg,
        RESULT_FRAME_OFFSET + 40,
    );
    let context_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &context_reg, 16);
    emit_typed_wrapper_result(
        ctx,
        "Dom\\Element",
        &context_reg,
        &handle_reg,
        true,
        None,
    )?;
    let object_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &object_reg,
        OBJECT_RESULT_OFFSET,
    );
    abi::emit_store_to_address(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        &object_reg,
        property_offset,
    );
    abi::emit_store_zero_to_address(
        ctx.emitter,
        &object_reg,
        property_offset + 8,
    );
    Ok(())
}

/// Materializes six flat libxml fields into an ordinary mutable PHP `LibXMLError`.
fn materialize_libxml_error_value_object(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let offsets = libxml_error_property_offsets(ctx)?;
    let failure = ctx.next_label("dom_libxml_error_value_failure");
    let complete = ctx.next_label("dom_libxml_error_value_complete");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", OBJECT_FIELD_COUNT_OFFSET)); // load the declared value-object field count
            ctx.emitter.instruction("cmp x9, #6");                              // does this descriptor contain every LibXMLError field exactly once?
            ctx.emitter
                .instruction(&format!("b.ne {}", failure));                     // reject an incompatible value-object schema
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", OBJECT_FIELDS_OFFSET)); // load the flat field-range start
            ctx.emitter.instruction("adds x10, x9, #6");                        // compute the exclusive field-range end with overflow detection
            ctx.emitter
                .instruction(&format!("b.cs {}", failure));                     // reject an overflowing field range
            ctx.emitter.instruction("ldr x11, [sp, #120]");                     // load the retained flat value count
            ctx.emitter.instruction("cmp x10, x11");                            // does the complete field range fit the retained result?
            ctx.emitter
                .instruction(&format!("b.hi {}", failure));                     // reject a field range outside native result storage
            ctx.emitter.instruction("ldr x11, [sp, #112]");                     // load the retained flat value-vector pointer
            ctx.emitter.instruction("add x10, x9, x9, lsl #1");                 // multiply the start index by three ABI words
            ctx.emitter.instruction("add x11, x11, x10, lsl #3");               // address the first 24-byte field record
            ctx.emitter
                .instruction(&format!("str x11, [sp, #{}]", OBJECT_FIELDS_OFFSET)); // retain the first field pointer across allocations
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp QWORD PTR [rsp + {}], 6", OBJECT_FIELD_COUNT_OFFSET)); // must this descriptor expose exactly six fields?
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject an incompatible value-object schema
            ctx.emitter
                .instruction(&format!("mov r10, QWORD PTR [rsp + {}]", OBJECT_FIELDS_OFFSET)); // load the flat field-range start
            ctx.emitter.instruction("mov r11, r10");                            // preserve the start for pointer materialization
            ctx.emitter.instruction("add r11, 6");                              // compute the exclusive field-range end
            ctx.emitter
                .instruction(&format!("jc {}", failure));                       // reject an overflowing field range
            ctx.emitter.instruction("cmp r11, QWORD PTR [rsp + 120]");          // does the complete field range fit the retained result?
            ctx.emitter
                .instruction(&format!("ja {}", failure));                       // reject a field range outside native result storage
            ctx.emitter.instruction("lea r10, [r10 + r10 * 2]");                // multiply the start index by three ABI words
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 112]");          // load the retained flat value-vector pointer
            ctx.emitter.instruction("lea r11, [r11 + r10 * 8]");                // address the first 24-byte field record
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [rsp + {}], r11", OBJECT_FIELDS_OFFSET)); // retain the first field pointer across allocations
        }
    }

    super::objects::emit_internal_value_object_allocation(ctx, "LibXMLError")?;
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        OBJECT_RESULT_OFFSET,
    );
    emit_libxml_error_integer_field(ctx, 0, offsets[0], &failure);
    emit_libxml_error_integer_field(ctx, 1, offsets[1], &failure);
    emit_libxml_error_integer_field(ctx, 2, offsets[2], &failure);
    emit_libxml_error_string_field(ctx, 3, offsets[3], &failure);
    emit_libxml_error_string_field(ctx, 4, offsets[4], &failure);
    emit_libxml_error_integer_field(ctx, 5, offsets[5], &failure);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        OBJECT_RESULT_OFFSET,
    );
    abi::emit_jump(ctx.emitter, &complete);
    ctx.emitter.label(&failure);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("b __rt_dom_bridge_failure"),  // contain a malformed native value-object descriptor
        Arch::X86_64 => ctx.emitter.instruction("jmp __rt_dom_bridge_failure"), // contain a malformed native value-object descriptor
    }
    ctx.emitter.label(&complete);
    Ok(())
}

/// Resolves and validates the physical property layout of PHP's `LibXMLError`.
fn libxml_error_property_offsets(ctx: &FunctionContext<'_>) -> Result<[usize; 6]> {
    let class_info = ctx
        .module
        .class_infos
        .get("LibXMLError")
        .ok_or_else(|| CodegenIrError::unsupported("unknown class LibXMLError"))?;
    let properties = [
        ("level", PhpType::Int),
        ("code", PhpType::Int),
        ("column", PhpType::Int),
        ("message", PhpType::Str),
        ("file", PhpType::Str),
        ("line", PhpType::Int),
    ];
    let mut offsets = [0; 6];
    for (index, (property, expected_type)) in properties.into_iter().enumerate() {
        let (_, (_, actual_type)) = class_info.visible_property(property).ok_or_else(|| {
            CodegenIrError::invalid_module(format!(
                "LibXMLError is missing declared property ${property}"
            ))
        })?;
        if actual_type.codegen_repr() != expected_type {
            return Err(CodegenIrError::invalid_module(format!(
                "LibXMLError::${property} has incompatible type {actual_type:?}"
            )));
        }
        offsets[index] = *class_info.property_offsets.get(property).ok_or_else(|| {
            CodegenIrError::invalid_module(format!(
                "LibXMLError::${property} has no object-layout offset"
            ))
        })?;
    }
    Ok(offsets)
}

/// Copies one integer field record into a declared `LibXMLError` property slot.
fn emit_libxml_error_integer_field(
    ctx: &mut FunctionContext<'_>,
    field_index: usize,
    property_offset: usize,
    failure: &str,
) {
    let field_offset = field_index * ABI_VALUE_SIZE;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", OBJECT_FIELDS_OFFSET)); // reload the first value-object field record
            ctx.emitter
                .instruction(&format!("ldr w10, [x9, #{}]", field_offset));     // load this field's ABI value tag
            ctx.emitter.instruction("cmp w10, #2");                             // must this LibXMLError field be an integer?
            ctx.emitter
                .instruction(&format!("b.ne {}", failure));                     // reject a mismatched field value tag
            ctx.emitter
                .instruction(&format!("ldr x10, [x9, #{}]", field_offset + 8)); // load the signed integer bit pattern
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", OBJECT_RESULT_OFFSET)); // reload the ordinary PHP object payload
            ctx.emitter
                .instruction(&format!("str x10, [x9, #{}]", property_offset));  // initialize the declared integer property
            ctx.emitter
                .instruction(&format!("str xzr, [x9, #{}]", property_offset + 8)); // clear the typed-property uninitialized marker
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r10, QWORD PTR [rsp + {}]", OBJECT_FIELDS_OFFSET)); // reload the first value-object field record
            ctx.emitter
                .instruction(&format!("cmp DWORD PTR [r10 + {}], 2", field_offset)); // must this LibXMLError field be an integer?
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject a mismatched field value tag
            ctx.emitter
                .instruction(&format!("mov r11, QWORD PTR [r10 + {}]", field_offset + 8)); // load the signed integer bit pattern
            ctx.emitter
                .instruction(&format!("mov r10, QWORD PTR [rsp + {}]", OBJECT_RESULT_OFFSET)); // reload the ordinary PHP object payload
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [r10 + {}], r11", property_offset)); // initialize the declared integer property
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [r10 + {}], 0", property_offset + 8)); // clear the typed-property uninitialized marker
        }
    }
}

/// Persists one borrowed byte field into a declared `LibXMLError` string slot.
fn emit_libxml_error_string_field(
    ctx: &mut FunctionContext<'_>,
    field_index: usize,
    property_offset: usize,
    failure: &str,
) {
    let field_offset = field_index * ABI_VALUE_SIZE;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldr x9, [sp, #{}]", OBJECT_FIELDS_OFFSET)); // reload the first value-object field record
            ctx.emitter
                .instruction(&format!("ldr w10, [x9, #{}]", field_offset));     // load this field's ABI value tag
            ctx.emitter.instruction("cmp w10, #4");                             // must this LibXMLError field be bytes?
            ctx.emitter
                .instruction(&format!("b.ne {}", failure));                     // reject a mismatched field value tag
            ctx.emitter
                .instruction(&format!("ldr x10, [x9, #{}]", field_offset + 8)); // load the byte-range offset
            ctx.emitter
                .instruction(&format!("ldr x11, [x9, #{}]", field_offset + 16)); // load the byte-range length
            ctx.emitter.instruction("ldr x1, [sp, #96]");                       // load the retained native byte-buffer pointer
            ctx.emitter.instruction("add x1, x1, x10");                         // address this exact string field
            ctx.emitter.instruction("adds x10, x10, x11");                      // compute the exclusive byte-range end with overflow detection
            ctx.emitter
                .instruction(&format!("b.cs {}", failure));                     // reject an overflowing byte range
            ctx.emitter.instruction("ldr x9, [sp, #104]");                      // load the retained native byte-buffer length
            ctx.emitter.instruction("cmp x10, x9");                             // does this byte range fit the retained result?
            ctx.emitter
                .instruction(&format!("b.hi {}", failure));                     // reject bytes outside native result storage
            ctx.emitter.instruction("mov x2, x11");                             // pass the exact byte length to string persistence
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov r10, QWORD PTR [rsp + {}]", OBJECT_FIELDS_OFFSET)); // reload the first value-object field record
            ctx.emitter
                .instruction(&format!("cmp DWORD PTR [r10 + {}], 4", field_offset)); // must this LibXMLError field be bytes?
            ctx.emitter
                .instruction(&format!("jne {}", failure));                      // reject a mismatched field value tag
            ctx.emitter
                .instruction(&format!("mov r11, QWORD PTR [r10 + {}]", field_offset + 8)); // load the byte-range offset
            ctx.emitter
                .instruction(&format!("mov rdx, QWORD PTR [r10 + {}]", field_offset + 16)); // load the byte-range length
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 96]");           // load the retained native byte-buffer pointer
            ctx.emitter.instruction("add rax, r11");                            // address this exact string field
            ctx.emitter.instruction("add r11, rdx");                            // compute the exclusive byte-range end
            ctx.emitter
                .instruction(&format!("jc {}", failure));                       // reject an overflowing byte range
            ctx.emitter.instruction("cmp r11, QWORD PTR [rsp + 104]");          // does this byte range fit the retained result?
            ctx.emitter
                .instruction(&format!("ja {}", failure));                       // reject bytes outside native result storage
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    let (pointer_reg, length_reg) = abi::string_result_regs(ctx.emitter);
    let object_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &object_reg,
        OBJECT_RESULT_OFFSET,
    );
    abi::emit_store_to_address(
        ctx.emitter,
        pointer_reg,
        &object_reg,
        property_offset,
    );
    abi::emit_store_to_address(
        ctx.emitter,
        length_reg,
        &object_reg,
        property_offset + 8,
    );
}

/// Requires one exact native result tag or transfers control to fatal ABI containment.
fn require_result_tag(ctx: &mut FunctionContext<'_>, tag: i64) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let tag_ok = ctx.next_label("dom_result_tag_ok");
            ctx.emitter.instruction("ldr w9, [sp, #60]");                       // load the native result value tag
            ctx.emitter
                .instruction(&format!("cmp w9, #{}", tag));                     // compare the result against the statically required ABI tag
            ctx.emitter
                .instruction(&format!("b.eq {}", tag_ok));                      // continue only when the native result matches its EIR type
            ctx.emitter.instruction("mov x0, #73");                             // classify a native/compiler result-tag mismatch
            ctx.emitter.instruction("b __rt_dom_bridge_failure_code");          // reject the result-contract mismatch
            ctx.emitter.label(&tag_ok);
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp DWORD PTR [rsp + 60], {}", tag));    // compare the native result against the statically required ABI tag
            let tag_ok = ctx.next_label("dom_result_tag_ok");
            ctx.emitter
                .instruction(&format!("je {}", tag_ok));                        // continue only when the native result matches its EIR type
            ctx.emitter.instruction("mov eax, 73");                             // classify a native/compiler result-tag mismatch
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure_code");        // reject the result-contract mismatch
            ctx.emitter.label(&tag_ok);
        }
    }
    Ok(())
}

/// Copies one result word into reserved storage that survives result-frame release.
fn stage_result_word(
    ctx: &mut FunctionContext<'_>,
    source_offset: usize,
    destination_offset: usize,
) {
    let scratch = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(ctx.emitter, &scratch, source_offset);
    abi::emit_store_to_sp(ctx.emitter, &scratch, destination_offset);
}

/// Copies a borrowed native byte result into owned Elephc string storage.
fn materialize_owned_result_string(ctx: &mut FunctionContext<'_>) -> Result<()> {
    require_result_tag(ctx, 4)?;
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        ptr_reg,
        RESULT_FRAME_OFFSET + 48,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        len_reg,
        RESULT_FRAME_OFFSET + 56,
    );
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    Ok(())
}

/// Stages the current two-word string result across native cleanup calls.
fn stage_current_string(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_store_to_sp(ctx.emitter, ptr_reg, TEMP_RESULT_LO_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, len_reg, TEMP_RESULT_HI_OFFSET);
}

/// Restores one staged owned string into the target's string result register pair.
fn restore_current_string(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, ptr_reg, TEMP_RESULT_LO_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, len_reg, TEMP_RESULT_HI_OFFSET);
}

/// Converts nullable/union ABI bytes, booleans, integers, or null into an owned Mixed cell.
fn materialize_mixed_result(
    ctx: &mut FunctionContext<'_>,
    opcode: u32,
    flags: u32,
    result_contract: &PhpType,
) -> Result<()> {
    let bytes = ctx.next_label("dom_result_mixed_bytes");
    let boolean = ctx.next_label("dom_result_mixed_bool");
    let integer = ctx.next_label("dom_result_mixed_int");
    let float = ctx.next_label("dom_result_mixed_float");
    let array = ctx.next_label("dom_result_mixed_array");
    let map = ctx.next_label("dom_result_mixed_map");
    let wrapper = ctx.next_label("dom_result_mixed_wrapper");
    let value_object = ctx.next_label("dom_result_mixed_value_object");
    let callable = ctx.next_label("dom_result_mixed_callable");
    let null = ctx.next_label("dom_result_mixed_null");
    let done = ctx.next_label("dom_result_mixed_done");
    let array_value_type = union_array_value_type(result_contract);
    let allows_map_result = matches!(opcode, 4426 | 4436 | 4438);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr w9, [sp, #60]");                       // load the native union result tag
            let mut alternatives = vec![
                (4, &bytes),
                (1, &boolean),
                (2, &integer),
                (3, &float),
                (8, &wrapper),
                (11, &value_object),
                (9, &callable),
                (0, &null),
            ];
            if array_value_type.is_some() {
                alternatives.push((5, &array));
                if allows_map_result {
                    alternatives.push((6, &map));
                }
            }
            for (tag, label) in alternatives {
                ctx.emitter
                    .instruction(&format!("cmp w9, #{}", tag));                 // compare against one supported union member tag
                ctx.emitter
                    .instruction(&format!("b.eq {}", label));                   // materialize the matching union member
            }
            ctx.emitter.instruction("b __rt_dom_bridge_failure");               // reject a result outside the statically declared union
        }
        Arch::X86_64 => {
            let mut alternatives = vec![
                (4, &bytes),
                (1, &boolean),
                (2, &integer),
                (3, &float),
                (8, &wrapper),
                (11, &value_object),
                (9, &callable),
                (0, &null),
            ];
            if array_value_type.is_some() {
                alternatives.push((5, &array));
                if allows_map_result {
                    alternatives.push((6, &map));
                }
            }
            for (tag, label) in alternatives {
                ctx.emitter
                    .instruction(&format!("cmp DWORD PTR [rsp + 60], {}", tag)); // compare against one supported union member tag
                ctx.emitter
                    .instruction(&format!("je {}", label));                     // materialize the matching union member
            }
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure");             // reject a result outside the statically declared union
        }
    }

    ctx.emitter.label(&bytes);
    materialize_owned_result_string(ctx)?;
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Str);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&boolean);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        RESULT_FRAME_OFFSET + 32,
    );
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&integer);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        RESULT_FRAME_OFFSET + 32,
    );
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&float);
    let scratch = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &scratch,
        RESULT_FRAME_OFFSET + 32,
    );
    // -- restore the exact native floating-point result --
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("fmov {}, {}", abi::float_result_reg(ctx.emitter), scratch)); // restore the exact returned IEEE-754 bits
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("movq {}, {}", abi::float_result_reg(ctx.emitter), scratch)); // restore the exact returned IEEE-754 bits
        }
    }
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Float);
    abi::emit_jump(ctx.emitter, &done);

    if let Some(array_value_type) = array_value_type {
        ctx.emitter.label(&array);
        materialize_result_array(ctx, &array_value_type)?;
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Array(Box::new(array_value_type.clone())),
        );
        abi::emit_jump(ctx.emitter, &done);

        if allows_map_result {
            ctx.emitter.label(&map);
            materialize_result_map(ctx, &PhpType::Str, &array_value_type)?;
            emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::AssocArray {
                    key: Box::new(PhpType::Str),
                    value: Box::new(array_value_type),
                },
            );
            abi::emit_jump(ctx.emitter, &done);
        }
    }

    ctx.emitter.label(&wrapper);
    if flags & FLAG_WRAPPER_RESULT != 0 {
        let eager_xpath_nodeset = matches!(
            opcode,
            MODERN_DOM_XPATH_QUERY_OPCODE | LEGACY_DOM_XPATH_QUERY_OPCODE
        );
        if eager_xpath_nodeset {
            materialize_xpath_nodeset_members(ctx)?;
        }
        let (class_name, requires_concrete_kind) =
            union_wrapper_class(result_contract)?;
        let context_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
        let handle_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
        abi::emit_load_temporary_stack_slot(ctx.emitter, &context_reg, 16);
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            &handle_reg,
            RESULT_FRAME_OFFSET + 32,
        );
        emit_typed_wrapper_result(
            ctx,
            &class_name,
            &context_reg,
            &handle_reg,
            requires_concrete_kind,
            Some(opcode),
        )?;
        if eager_xpath_nodeset {
            attach_xpath_nodeset_owner(ctx);
        }
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Object(class_name),
        );
        abi::emit_jump(ctx.emitter, &done);
    } else {
        emit_bridge_failure_jump(ctx);
    }

    ctx.emitter.label(&value_object);
    if flags & FLAG_VALUE_OBJECT_RESULT != 0 {
        let class_name = union_object_class(result_contract)?;
        if !crate::internal_extensions::is_native_value_object_class(&class_name) {
            return Err(CodegenIrError::invalid_module(format!(
                "unsupported native PHP value-object class {class_name}"
            )));
        }
        stage_result_word(
            ctx,
            RESULT_FRAME_OFFSET + 32,
            OBJECT_FIELDS_OFFSET,
        );
        stage_result_word(
            ctx,
            RESULT_FRAME_OFFSET + 40,
            OBJECT_FIELD_COUNT_OFFSET,
        );
        materialize_libxml_error_value_object(ctx)?;
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Object(class_name),
        );
        abi::emit_jump(ctx.emitter, &done);
    } else {
        emit_bridge_failure_jump(ctx);
    }

    ctx.emitter.label(&callable);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        RESULT_FRAME_OFFSET + 32,
    );
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Callable);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&null);
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        crate::codegen::NULL_SENTINEL,
    );
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
    ctx.emitter.label(&done);
    Ok(())
}

/// Returns the declared value type carried by one generic PHP array result contract.
fn union_array_value_type(result_type: &PhpType) -> Option<PhpType> {
    match result_type {
        PhpType::Array(element) => Some((**element).clone()),
        PhpType::AssocArray { value, .. } => Some((**value).clone()),
        PhpType::Union(members) => {
            let values = members
                .iter()
                .filter_map(|member| match member {
                    PhpType::Array(element) => Some((**element).clone()),
                    PhpType::AssocArray { value, .. } => Some((**value).clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let first = values.first().cloned()?;
            if values.iter().any(|value| value != &first) {
                return Some(PhpType::Mixed);
            }
            Some(first)
        }
        _ => None,
    }
}

/// Returns one same-family native-wrapper fallback and whether its contract is ambiguous.
fn union_wrapper_class(result_type: &PhpType) -> Result<(String, bool)> {
    if let PhpType::Object(class_name) = result_type.codegen_repr() {
        if !crate::internal_extensions::is_native_wrapper_class(&class_name) {
            return Err(CodegenIrError::unsupported(
                "bridge-handle result names an ordinary object",
            ));
        }
        return Ok((class_name, false));
    }
    let PhpType::Union(members) = result_type else {
        return Err(CodegenIrError::invalid_module(
            "bridge-handle mixed result is not an object or union",
        ));
    };
    let classes = members
        .iter()
        .filter_map(|member| {
            if let PhpType::Object(class_name) = member.codegen_repr() {
                Some(class_name)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let class_name = classes.first().cloned().ok_or_else(|| {
        CodegenIrError::invalid_module("bridge-handle union has no object member")
    })?;
    let modern = class_name.trim_start_matches('\\').starts_with("Dom\\");
    if classes.iter().any(|candidate| {
        !crate::internal_extensions::is_native_wrapper_class(candidate)
            || candidate.trim_start_matches('\\').starts_with("Dom\\") != modern
    }) {
        return Err(CodegenIrError::unsupported(
            "bridge-handle union mixes native-wrapper families or ordinary objects",
        ));
    }
    Ok((class_name, classes.len() > 1))
}

/// Returns the single PHP object class declared by one object or union result.
fn union_object_class(result_type: &PhpType) -> Result<String> {
    if let PhpType::Object(class_name) = result_type.codegen_repr() {
        return Ok(class_name);
    }
    let PhpType::Union(members) = result_type else {
        return Err(CodegenIrError::invalid_module(
            "value-object mixed result is not an object or union",
        ));
    };
    let classes = members
        .iter()
        .filter_map(|member| {
            if let PhpType::Object(class_name) = member.codegen_repr() {
                Some(class_name)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let class_name = classes.first().cloned().ok_or_else(|| {
        CodegenIrError::invalid_module("value-object union has no object member")
    })?;
    if classes.len() > 1 {
        return Err(CodegenIrError::unsupported(
            "value-object union with multiple object classes",
        ));
    }
    Ok(class_name)
}

/// Materializes a cached or newly allocated concrete DOM wrapper from its stable native kind.
fn emit_typed_wrapper_result(
    ctx: &mut FunctionContext<'_>,
    static_class: &str,
    context_reg: &str,
    handle_reg: &str,
    requires_concrete_kind: bool,
    opcode: Option<u32>,
) -> Result<()> {
    let modern = static_class.trim_start_matches('\\').starts_with("Dom\\");
    let mut candidates = if modern {
        crate::internal_extensions::MODERN_DOM_WRAPPER_KINDS.to_vec()
    } else {
        crate::internal_extensions::LEGACY_DOM_WRAPPER_KINDS.to_vec()
    };
    if modern && opcode == Some(4096) {
        candidates.extend([(101, "DOMElement"), (102, "DOMAttr")]);
    }
    let fallback = ctx.next_label("dom_wrapper_kind_static");
    let done = ctx.next_label("dom_wrapper_kind_done");
    let labels = candidates
        .iter()
        .map(|_| ctx.next_label("dom_wrapper_kind_concrete"))
        .collect::<Vec<_>>();
    let mut registered_candidates = ctx
        .module
        .class_infos
        .iter()
        .filter(|(class_name, _)| {
            crate::internal_extensions::is_native_wrapper_descendant(
                &ctx.module.class_infos,
                class_name,
            )
        })
        .map(|(class_name, class_info)| (class_info.class_id, class_name.clone()))
        .collect::<Vec<_>>();
    registered_candidates.sort_by_key(|(class_id, _)| *class_id);
    let registered_labels = registered_candidates
        .iter()
        .map(|_| ctx.next_label("dom_wrapper_kind_registered"))
        .collect::<Vec<_>>();
    let registered_dispatch = ctx.next_label("dom_wrapper_kind_registered_dispatch");
    let kind_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &kind_reg,
        RESULT_FRAME_OFFSET + 40,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(
                "tbnz {}, #63, {}",
                kind_reg, registered_dispatch
            )); // high-bit discriminators carry compiler class ids selected by registerNodeClass()
            ctx.emitter
                .instruction(&format!("cbz {}, {}", kind_reg, fallback));       // a zero discriminator retains the statically declared wrapper class
            for ((kind, _), label) in candidates.iter().zip(&labels) {
                ctx.emitter
                    .instruction(&format!("cmp {}, #{}", kind_reg, kind));      // compare one stable native wrapper discriminator
                ctx.emitter
                    .instruction(&format!("b.eq {}", label));                   // allocate the matching concrete PHP DOM wrapper
            }
            ctx.emitter.instruction("b __rt_dom_bridge_failure");               // reject an unknown native wrapper discriminator
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", kind_reg, kind_reg)); // inspect the mapped-class high bit before native kind dispatch
            ctx.emitter
                .instruction(&format!("js {}", registered_dispatch));           // negative u64 values carry registerNodeClass() compiler class ids
            ctx.emitter
                .instruction(&format!("test {}, {}", kind_reg, kind_reg));      // does this result carry a concrete native wrapper discriminator?
            ctx.emitter
                .instruction(&format!("jz {}", fallback));                      // a zero discriminator retains the statically declared wrapper class
            for ((kind, _), label) in candidates.iter().zip(&labels) {
                ctx.emitter
                    .instruction(&format!("cmp {}, {}", kind_reg, kind));       // compare one stable native wrapper discriminator
                ctx.emitter
                    .instruction(&format!("je {}", label));                     // allocate the matching concrete PHP DOM wrapper
            }
            ctx.emitter.instruction("jmp __rt_dom_bridge_failure");             // reject an unknown native wrapper discriminator
        }
    }

    ctx.emitter.label(&registered_dispatch);
    abi::emit_push_reg_pair(ctx.emitter, context_reg, handle_reg);
    let class_id_reg = abi::temp_int_reg(ctx.emitter.target);
    for ((class_id, _), label) in registered_candidates.iter().zip(&registered_labels) {
        abi::emit_load_int_immediate(
            ctx.emitter,
            class_id_reg,
            (crate::internal_extensions::DOM_USER_WRAPPER_MARKER | class_id) as i64,
        );
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter
                    .instruction(&format!("cmp {}, {}", kind_reg, class_id_reg)); // compare the complete mapped user-wrapper discriminator
                ctx.emitter
                    .instruction(&format!("b.eq {}", label));                   // allocate the bridge-selected registered PHP class
            }
            Arch::X86_64 => {
                ctx.emitter
                    .instruction(&format!("cmp {}, {}", kind_reg, class_id_reg)); // compare the complete mapped user-wrapper discriminator
                ctx.emitter
                    .instruction(&format!("je {}", label));                     // allocate the bridge-selected registered PHP class
            }
        }
    }
    abi::emit_pop_reg_pair(ctx.emitter, context_reg, handle_reg);
    emit_bridge_failure_jump(ctx);

    ctx.emitter.label(&fallback);
    if requires_concrete_kind {
        emit_bridge_failure_jump(ctx);
    } else {
        super::objects::emit_internal_extension_wrapper_value(
            ctx,
            static_class,
            context_reg,
            handle_reg,
        )?;
        abi::emit_jump(ctx.emitter, &done);
    }

    for ((_, class_name), label) in candidates.iter().zip(labels) {
        ctx.emitter.label(&label);
        super::objects::emit_internal_extension_wrapper_value(
            ctx,
            class_name,
            context_reg,
            handle_reg,
        )?;
        abi::emit_jump(ctx.emitter, &done);
    }
    for ((_, class_name), label) in registered_candidates.iter().zip(registered_labels) {
        ctx.emitter.label(&label);
        abi::emit_pop_reg_pair(ctx.emitter, context_reg, handle_reg);
        super::objects::emit_registered_internal_extension_wrapper_value(
            ctx,
            class_name,
            context_reg,
            handle_reg,
        )?;
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Transfers a malformed native union member to the shared fatal containment helper.
fn emit_bridge_failure_jump(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("b __rt_dom_bridge_failure"),  // contain an undeclared native union member
        Arch::X86_64 => ctx.emitter.instruction("jmp __rt_dom_bridge_failure"), // contain an undeclared native union member
    }
}

/// Releases the independent native result frame and the temporary flat request.
fn emit_release_native_call_state(ctx: &mut FunctionContext<'_>) -> Result<()> {
    emit_release_prepared_xpath_callback_value(ctx);
    emit_release_temporary_callable_descriptor(ctx);
    let skip_release = ctx.next_label("dom_result_release_skip");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "x1",
                RESULT_FRAME_OFFSET + 24,
            );
            ctx.emitter
                .instruction(&format!("cbz x1, {}", skip_release));             // pointer-free error frames retain no native result storage
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 16);
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(
                ctx.emitter,
                "rsi",
                RESULT_FRAME_OFFSET + 24,
            );
            ctx.emitter.instruction("test rsi, rsi");                           // did the bridge retain a native result frame?
            ctx.emitter
                .instruction(&format!("jz {}", skip_release));                  // pointer-free error frames retain no native result storage
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 16);
        }
    }
    let release = ctx
        .emitter
        .target
        .extern_symbol("elephc_dom_result_release");
    abi::emit_call_label(ctx.emitter, &release);
    ctx.emitter.label(&skip_release);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0,
    );
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    Ok(())
}

/// Releases a fully encoded request when re-entry makes its pending iterator move stale.
fn emit_release_abandoned_native_request_state(ctx: &mut FunctionContext<'_>) {
    emit_release_prepared_xpath_callback_value(ctx);
    emit_release_temporary_callable_descriptor(ctx);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0,
    );
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
}

/// Releases descriptors and storage owned by one prepared XPath callback request value.
fn emit_release_prepared_xpath_callback_value(ctx: &mut FunctionContext<'_>) {
    let skip = ctx.next_label("dom_prepared_xpath_callback_skip");
    let loop_label = ctx.next_label("dom_prepared_xpath_callback_release_loop");
    let release_done = ctx.next_label("dom_prepared_xpath_callback_release_done");
    let plan_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        plan_reg,
        PREPARED_XPATH_CALLBACK_VALUE_OFFSET,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", plan_reg, skip));    // skip calls that did not prepare XPath callback arrays
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", plan_reg, plan_reg));      // did this call prepare an XPath callback value?
            ctx.emitter
                .instruction(&format!("jz {}", skip));                          // skip ordinary internal-extension request cleanup
        }
    }
    let count_symbol = ctx
        .emitter
        .target
        .extern_symbol("elephc_dom_prepared_xpath_callback_descriptor_count");
    abi::emit_call_label(ctx.emitter, &count_symbol);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        RUNTIME_VALUE_MEASURE_OFFSET,
    );
    store_stack_immediate(ctx, RUNTIME_VALUE_MEASURE_OFFSET + 8, 0);

    ctx.emitter.label(&loop_label);
    let index_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let count_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &index_reg,
        RUNTIME_VALUE_MEASURE_OFFSET + 8,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &count_reg,
        RUNTIME_VALUE_MEASURE_OFFSET,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", index_reg, count_reg));     // have all prepared callable descriptors been released?
            ctx.emitter
                .instruction(&format!("b.hs {}", release_done));                // continue with plan destruction after the final descriptor
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", index_reg, count_reg));     // have all prepared callable descriptors been released?
            ctx.emitter
                .instruction(&format!("jae {}", release_done));                 // continue with plan destruction after the final descriptor
        }
    }
    let second_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        plan_reg,
        PREPARED_XPATH_CALLBACK_VALUE_OFFSET,
    );
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        second_arg,
        RUNTIME_VALUE_MEASURE_OFFSET + 8,
    );
    let at_symbol = ctx
        .emitter
        .target
        .extern_symbol("elephc_dom_prepared_xpath_callback_descriptor_at");
    abi::emit_call_label(ctx.emitter, &at_symbol);
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        &index_reg,
        RUNTIME_VALUE_MEASURE_OFFSET + 8,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("add {}, {}, #1", index_reg, index_reg)); // advance to the next prepared callable descriptor
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("add {}, 1", index_reg));                 // advance to the next prepared callable descriptor
        }
    }
    abi::emit_store_to_sp(
        ctx.emitter,
        &index_reg,
        RUNTIME_VALUE_MEASURE_OFFSET + 8,
    );
    abi::emit_jump(ctx.emitter, &loop_label);

    ctx.emitter.label(&release_done);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        plan_reg,
        PREPARED_XPATH_CALLBACK_VALUE_OFFSET,
    );
    let free_symbol = ctx
        .emitter
        .target
        .extern_symbol("elephc_dom_prepared_xpath_callback_value_free");
    abi::emit_call_label(ctx.emitter, &free_symbol);
    store_stack_immediate(ctx, PREPARED_XPATH_CALLBACK_VALUE_OFFSET, 0);
    ctx.emitter.label(&skip);
}

/// Releases a receiver-bound descriptor synthesized only for the current bridge call.
fn emit_release_temporary_callable_descriptor(ctx: &mut FunctionContext<'_>) {
    let skip_release = ctx.next_label("dom_temporary_callable_release_skip");
    let descriptor_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        descriptor_reg,
        TEMP_CALLABLE_DESCRIPTOR_OFFSET,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!(
                "cbz {}, {}",
                descriptor_reg, skip_release
            )); // persistent or absent callables need no temporary release
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", descriptor_reg, descriptor_reg)); // is there a receiver-bound temporary descriptor?
            ctx.emitter
                .instruction(&format!("jz {}", skip_release));                  // skip persistent or absent callables
        }
    }
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    store_stack_immediate(ctx, TEMP_CALLABLE_DESCRIPTOR_OFFSET, 0);
    ctx.emitter.label(&skip_release);
}

/// Stores one immediate word into reserved temporary-stack storage.
fn store_stack_immediate(ctx: &mut FunctionContext<'_>, offset: usize, value: i64) {
    let scratch = abi::secondary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_int_immediate(ctx.emitter, &scratch, value);
    abi::emit_store_to_sp(ctx.emitter, &scratch, offset);
}

/// Writes one 32-bit immediate field through a request base pointer.
fn emit_store_u32_immediate(
    ctx: &mut FunctionContext<'_>,
    base_reg: &str,
    offset: usize,
    value: i64,
) {
    let scratch = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    abi::emit_load_int_immediate(ctx.emitter, &scratch, value);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("str w{}, [{}, #{}]", &scratch[1..], base_reg, offset)); // write one fixed-width ABI u32 field
        }
        Arch::X86_64 => {
            let register32 = match scratch.as_str() {
                "r10" => "r10d",
                "r11" => "r11d",
                "rcx" => "ecx",
                "rdx" => "edx",
                "r8" => "r8d",
                "r9" => "r9d",
                other => other,
            };
            ctx.emitter.instruction(&format!(
                "mov DWORD PTR [{} + {}], {}",
                base_reg, offset, register32
            )); // write one fixed-width ABI u32 field
        }
    }
}

/// Stores the current result registers into an instruction's allocated SSA home.
fn store_instruction_result(
    ctx: &mut FunctionContext<'_>,
    instruction: &Instruction,
) -> Result<()> {
    let result = instruction.result.ok_or_else(|| {
        CodegenIrError::invalid_module("internal extension value result is absent")
    })?;
    ctx.store_result_value(result)
}
