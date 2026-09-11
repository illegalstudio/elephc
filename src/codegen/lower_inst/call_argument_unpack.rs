//! Purpose:
//! Lowers typed dynamic call-unpack validation and collection operations.
//! Keeps PHP errors, target ABI registers, and accumulator writeback in one backend leaf.
//!
//! Called from:
//! - `super::runtime_calls::lower()` for `call_argument.*` runtime targets.
//!
//! Key details:
//! - Validation runs before either private accumulator is mutated.
//! - Collectors return the current destination pointer so its SSA home and exception guard can
//!   be refreshed before any later throwing operation.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, RuntimeCallTarget, ValueId};
use crate::types::PhpType;

use super::expect_operand;

const UNPACK_TYPE_ERROR_PREFIX: &str = "Only arrays and Traversables can be unpacked, ";
const UNPACK_TYPE_ERROR_SUFFIX: &str = " given";

/// Lowers one call-unpack target through the shared runtime helper ABI.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeCallTarget,
) -> Result<()> {
    let destination = expect_operand(inst, 0)?;
    let source = expect_operand(inst, 1)?;
    require_hash_destination(ctx, inst, destination)?;
    require_mixed_source(ctx, inst, source)?;
    load_operands(ctx, destination, source)?;

    match target {
        RuntimeCallTarget::CallArgumentValidateUnpack => {
            abi::emit_call_label(ctx.emitter, "__rt_call_argument_validate_unpack");
            emit_validation_result(ctx);
            Ok(())
        }
        RuntimeCallTarget::CallArgumentCollectPositionals => {
            abi::emit_call_label(ctx.emitter, "__rt_call_argument_collect_positionals");
            ctx.store_result_value(destination)
        }
        RuntimeCallTarget::CallArgumentCollectNamed => {
            abi::emit_call_label(ctx.emitter, "__rt_call_argument_collect_named");
            ctx.store_result_value(destination)
        }
        _ => Err(CodegenIrError::invalid_module(format!(
            "unexpected call-unpack runtime target {:?}",
            target
        ))),
    }
}

/// Verifies the physical destination shape expected by the private hash accumulators.
fn require_hash_destination(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    destination: ValueId,
) -> Result<()> {
    let actual = ctx.value_php_type(destination)?.codegen_repr();
    if matches!(actual, PhpType::AssocArray { .. }) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "typed runtime {} expected an associative-array destination, got {:?}",
        inst.op.name(),
        actual
    )))
}

/// Verifies that dynamic unpack input uses the boxed Mixed runtime representation.
fn require_mixed_source(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    source: ValueId,
) -> Result<()> {
    let actual = ctx.value_php_type(source)?.codegen_repr();
    if actual == PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "typed runtime {} expected a boxed Mixed source, got {:?}",
        inst.op.name(),
        actual
    )))
}

/// Materializes the stable two-argument helper ABI on the active target.
fn load_operands(
    ctx: &mut FunctionContext<'_>,
    destination: ValueId,
    source: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(destination, "x0")?;
            ctx.load_value_to_reg(source, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(destination, "rdi")?;
            ctx.load_value_to_reg(source, "rsi")?;
        }
    }
    Ok(())
}

/// Turns the validator's compact status into PHP's catchable call-unpack exceptions.
///
/// Status zero succeeds. Status one carries a concrete runtime tag and low payload word,
/// status two carries a borrowed duplicate-name string, status three reports a positional entry
/// following a named entry inside one source, and status four rejects the unsupported ordering
/// where a later dynamic source contributes positional arguments after an earlier named source.
fn emit_validation_result(ctx: &mut FunctionContext<'_>) {
    let ok = ctx.next_label("call_argument_unpack_valid");
    let non_array = ctx.next_label("call_argument_unpack_non_array");
    let duplicate = ctx.next_label("call_argument_unpack_duplicate");
    let positional_after_named = ctx.next_label("call_argument_unpack_positional_after_named");
    let unsupported_cross_source = ctx.next_label("call_argument_unpack_cross_source_order");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", ok));                // status zero means validation completed without mutating either accumulator
            ctx.emitter.instruction("cmp x0, #1");                              // does the source carry a non-array runtime tag?
            ctx.emitter.instruction(&format!("b.eq {}", non_array));            // map the runtime tag to PHP's unpack TypeError
            ctx.emitter.instruction("cmp x0, #2");                              // did a string key overwrite an earlier named argument?
            ctx.emitter.instruction(&format!("b.eq {}", duplicate));            // build the duplicate-name Error from the borrowed key
            ctx.emitter.instruction("cmp x0, #3");                              // did one source contain a positional key after a named key?
            ctx.emitter.instruction(&format!("b.eq {}", positional_after_named)); // preserve PHP's within-source unpack Error
            ctx.emitter.instruction(&format!("b {}", unsupported_cross_source)); // status four is the explicit unsupported cross-source order
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // status zero means validation completed without mutating either accumulator
            ctx.emitter.instruction(&format!("jz {}", ok));                     // continue lowering after a valid unpack source
            ctx.emitter.instruction("cmp rax, 1");                              // does the source carry a non-array runtime tag?
            ctx.emitter.instruction(&format!("je {}", non_array));              // map the runtime tag to PHP's unpack TypeError
            ctx.emitter.instruction("cmp rax, 2");                              // did a string key overwrite an earlier named argument?
            ctx.emitter.instruction(&format!("je {}", duplicate));              // build the duplicate-name Error from the borrowed key
            ctx.emitter.instruction("cmp rax, 3");                              // did one source contain a positional key after a named key?
            ctx.emitter.instruction(&format!("je {}", positional_after_named)); // preserve PHP's within-source unpack Error
            ctx.emitter.instruction(&format!("jmp {}", unsupported_cross_source)); // status four is the explicit unsupported cross-source order
        }
    }

    ctx.emitter.label(&non_array);
    emit_non_array_type_error(ctx);

    ctx.emitter.label(&duplicate);
    emit_duplicate_named_error(ctx);

    ctx.emitter.label(&positional_after_named);
    super::exceptions::emit_error(
        ctx,
        "Cannot use positional argument after named argument during unpacking",
    );

    ctx.emitter.label(&unsupported_cross_source);
    super::exceptions::emit_error(
        ctx,
        "elephc does not support positional unpacking after named unpacking in dynamically resolved calls",
    );

    ctx.emitter.label(&ok);
}

/// Emits the tag-specific wording for a non-array dynamic source.
fn emit_non_array_type_error(ctx: &mut FunctionContext<'_>) {
    let bool_case = ctx.next_label("call_argument_unpack_type_bool");
    let object_case = ctx.next_label("call_argument_unpack_type_object");
    let cases = [
        (0_u64, "int"),
        (1, "string"),
        (2, "float"),
        (8, "null"),
        (9, "resource"),
        (10, "Closure"),
    ]
    .map(|(tag, name)| (tag, name, ctx.next_label("call_argument_unpack_type")));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x1, #3");                              // bool needs its payload to distinguish PHP's true and false names
            ctx.emitter.instruction(&format!("b.eq {}", bool_case));            // branch to the bool payload dispatch
            for (tag, _, label) in &cases {
                ctx.emitter.instruction(&format!("cmp x1, #{}", tag));          // compare the rejected Mixed tag with one PHP scalar type
                ctx.emitter.instruction(&format!("b.eq {}", label));            // select its exact PHP type-name message
            }
            ctx.emitter.instruction("cmp x1, #6");                              // object errors name the concrete runtime class
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // load that class name from object metadata
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rdi, 3");                              // bool needs its payload to distinguish PHP's true and false names
            ctx.emitter.instruction(&format!("je {}", bool_case));              // branch to the bool payload dispatch
            for (tag, _, label) in &cases {
                ctx.emitter.instruction(&format!("cmp rdi, {}", tag));          // compare the rejected Mixed tag with one PHP scalar type
                ctx.emitter.instruction(&format!("je {}", label));              // select its exact PHP type-name message
            }
            ctx.emitter.instruction("cmp rdi, 6");                              // object errors name the concrete runtime class
            ctx.emitter.instruction(&format!("je {}", object_case));            // load that class name from object metadata
        }
    }
    emit_unpack_type_error(ctx, "object");
    for (_, name, label) in &cases {
        ctx.emitter.label(label);
        emit_unpack_type_error(ctx, name);
    }
    ctx.emitter.label(&object_case);
    emit_object_unpack_type_error(ctx);
    ctx.emitter.label(&bool_case);
    let true_case = ctx.next_label("call_argument_unpack_type_true");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x2, {}", true_case));        // nonzero bool payloads are PHP true
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rsi, rsi");                           // inspect the rejected bool payload
            ctx.emitter.instruction(&format!("jnz {}", true_case));             // nonzero bool payloads are PHP true
        }
    }
    emit_unpack_type_error(ctx, "false");
    ctx.emitter.label(&true_case);
    emit_unpack_type_error(ctx, "true");
}

/// Emits one static non-array unpack TypeError.
fn emit_unpack_type_error(ctx: &mut FunctionContext<'_>, type_name: &str) {
    super::exceptions::emit_type_error(
        ctx,
        &format!("{UNPACK_TYPE_ERROR_PREFIX}{type_name}{UNPACK_TYPE_ERROR_SUFFIX}"),
    );
}

/// Builds the dynamic duplicate-name Error around the validator's borrowed key string.
fn emit_duplicate_named_error(ctx: &mut FunctionContext<'_>) {
    let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
    let (detail_ptr, detail_len) = validator_detail_regs(ctx.emitter.target.arch);
    if (detail_ptr, detail_len) != (text_ptr, text_len) {
        ctx.emitter.instruction(&format!("mov {}, {}", text_ptr, detail_ptr));  // move the validator key pointer into the string-result register
        ctx.emitter.instruction(&format!("mov {}, {}", text_len, detail_len));  // move the validator key length into the string-result register
    }
    emit_dynamic_message(
        ctx,
        b"Named parameter $",
        b" overwrites previous argument",
    );
    super::exceptions::emit_error_from_string_result(ctx);
}

/// Names a rejected object source from its runtime class metadata.
fn emit_object_unpack_type_error(ctx: &mut FunctionContext<'_>) {
    let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x2]");                            // load the rejected object's runtime class id
            abi::emit_symbol_address(ctx.emitter, "x10", "_class_name_entries");
            ctx.emitter.instruction("add x10, x10, x9, lsl #4");                // address the matching class-name metadata row
            ctx.emitter.instruction(&format!("ldr {}, [x10]", text_ptr));       // borrow the concrete class-name pointer
            ctx.emitter.instruction(&format!("ldr {}, [x10, #8]", text_len));   // borrow the concrete class-name byte length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rsi]");                 // load the rejected object's runtime class id
            abi::emit_symbol_address(ctx.emitter, "r10", "_class_name_entries");
            ctx.emitter.instruction("shl r9, 4");                               // scale the class id to its metadata row
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [r10 + r9]", text_ptr)); // borrow the concrete class-name pointer
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [r10 + r9 + 8]", text_len)); // borrow the concrete class-name byte length
        }
    }
    emit_dynamic_message(ctx, UNPACK_TYPE_ERROR_PREFIX.as_bytes(), UNPACK_TYPE_ERROR_SUFFIX.as_bytes());
    super::exceptions::emit_type_error_from_string_result(ctx);
}

/// Wraps the active borrowed string result in static prefix and suffix fragments.
///
/// The first concat can outgrow the shared scratch buffer, so its result is persisted and kept
/// as an explicit owner across the second concat. The complete result is parked while that
/// intermediate owner is released, then persisted for transfer into the Throwable object.
fn emit_dynamic_message(ctx: &mut FunctionContext<'_>, prefix: &[u8], suffix: &[u8]) {
    let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
    let (right_ptr, right_len) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    let (prefix, prefix_len) = ctx.data.add_string(prefix);
    let (suffix, suffix_len) = ctx.data.add_string(suffix);
    ctx.emitter.instruction(&format!("mov {}, {}", right_ptr, text_ptr));       // move the duplicate name into concat's right operand
    ctx.emitter.instruction(&format!("mov {}, {}", right_len, text_len));       // preserve the duplicate-name byte length
    abi::emit_symbol_address(ctx.emitter, text_ptr, &prefix);
    abi::emit_load_int_immediate(ctx.emitter, text_len, prefix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    abi::emit_push_reg_pair(ctx.emitter, text_ptr, text_len);
    abi::emit_symbol_address(ctx.emitter, right_ptr, &suffix);
    abi::emit_load_int_immediate(ctx.emitter, right_len, suffix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    abi::emit_push_reg_pair(ctx.emitter, text_ptr, text_len);
    let owner_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, owner_reg, 16);
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    abi::emit_pop_reg_pair(ctx.emitter, text_ptr, text_len);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
}

/// Returns the validator's status-adjacent detail register pair.
///
/// This is intentionally distinct from the string-result pair on x86_64, where `rax` carries the
/// status and therefore cannot simultaneously carry a duplicate-name pointer.
fn validator_detail_regs(arch: Arch) -> (&'static str, &'static str) {
    match arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi"),
    }
}
