//! Purpose:
//! Lowers typed EIR runtime operations after target selection and value placement.
//! Owns concrete helper symbols and physical calling-convention materialization.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_runtime_call()` for typed `RuntimeCall` immediates.
//!
//! Key details:
//! - PHP builtin names never participate in dispatch.
//! - Every typed call validates its EIR signature before emitting a helper call.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, RuntimeCallTarget, UnaryStringRuntime};
use crate::types::PhpType;

use super::{expect_operand, store_if_result};

/// Lowers one typed runtime operation through its target-specific helper ABI.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeCallTarget,
) -> Result<()> {
    match target {
        RuntimeCallTarget::ArrayFetchForWrite => {
            super::lower_array_fetch_for_write_runtime_call(ctx, inst)
        }
        RuntimeCallTarget::MixedCellPromoteToHash(sort)
        | RuntimeCallTarget::MixedCellPromoteAttachedToHash(sort) => {
            lower_mixed_cell_promote_to_hash(ctx, inst, sort)
        }
        RuntimeCallTarget::MixedCellClone => lower_mixed_cell_clone(ctx, inst),
        RuntimeCallTarget::DateSerializeFinalize => lower_date_serialize_finalize(ctx, inst),
        RuntimeCallTarget::MixedToArrayReturn
        | RuntimeCallTarget::DateSerializeMixedToArrayReturn => {
            lower_mixed_to_array_return(ctx, inst)
        }
        RuntimeCallTarget::UnaryString(runtime) => lower_unary_string(ctx, inst, runtime),
        RuntimeCallTarget::Pcntl(target) => {
            crate::codegen::lower_inst::builtins::pcntl::lower(ctx, inst, target)
        }
        RuntimeCallTarget::Function(target) => super::runtime_functions::lower(ctx, inst, target),
        RuntimeCallTarget::ProfiledFunction { target, .. } => {
            super::runtime_functions::lower(ctx, inst, target)
        }
    }
}

/// Converts and consumes one boxed Mixed result at a declared `array` boundary.
///
/// A valid indexed/hash payload gains the return reference before its source Mixed cell is
/// released. An invalid tag releases the cell before raising the PHP return-contract TypeError,
/// so an exception cannot strand an SSA-only owner outside the frame cleanup ledger.
fn lower_mixed_to_array_return(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime return.mixed_to_array expected 1 operand, got {}",
            inst.operands.len(),
        )));
    }
    let source = expect_operand(inst, 0)?;
    let source_ty = ctx.value_php_type(source)?.codegen_repr();
    let result_ty = inst.result_php_type.codegen_repr();
    if source_ty != PhpType::Mixed
        || !matches!(&result_ty, PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed)
    {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime return.mixed_to_array requires Mixed -> array<mixed>, got {:?} -> {:?}",
            source_ty, result_ty
        )));
    }

    let source_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_result(source)?;
    abi::emit_push_reg(ctx.emitter, source_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let hash_label = ctx.next_label("date_serialize_mixed_return_hash");
    let invalid_label = ctx.next_label("date_serialize_mixed_return_invalid");
    let done_label = ctx.next_label("date_serialize_mixed_return_done");
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #5");                              // does the owned Mixed cell hold an associative hash result?
            ctx.emitter.instruction(&format!("b.eq {}", hash_label));          // preserve a hash through PHP's generic array return contract
            ctx.emitter.instruction("cmp x0, #4");                              // does the owned Mixed cell hold an indexed array result?
            ctx.emitter.instruction(&format!("b.ne {}", invalid_label));       // release invalid source ownership before throwing TypeError
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed indexed-array payload to its Mixed-slot normalizer
            ctx.emitter.instruction("ldr x1, [x0, #-8]");                       // load indexed-array metadata before normalizing element storage
            ctx.emitter.instruction("lsr x1, x1, #8");                          // move indexed-array value-type bits into the helper argument register
            ctx.emitter.instruction("and x1, x1, #0x7f");                       // isolate the runtime value-type tag for the normalizer
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
            release_consumed_mixed_to_array_source(ctx);
            ctx.emitter.instruction(&format!("b {}", done_label));             // return the retained indexed-array payload
            ctx.emitter.label(&hash_label);
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed associative payload to its Mixed-value normalizer
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            abi::emit_incref_if_refcounted(
                ctx.emitter,
                &PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed),
                    value: Box::new(PhpType::Mixed),
                },
            );
            release_consumed_mixed_to_array_source(ctx);
            ctx.emitter.instruction(&format!("b {}", done_label));             // return the retained associative payload
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 5");                              // does the owned Mixed cell hold an associative hash result?
            ctx.emitter.instruction(&format!("je {}", hash_label));            // preserve a hash through PHP's generic array return contract
            ctx.emitter.instruction("cmp rax, 4");                              // does the owned Mixed cell hold an indexed array result?
            ctx.emitter.instruction(&format!("jne {}", invalid_label));        // release invalid source ownership before throwing TypeError
            ctx.emitter.instruction("mov rsi, QWORD PTR [rdi - 8]");            // load indexed-array metadata before normalizing element storage
            ctx.emitter.instruction("shr rsi, 8");                              // move indexed-array value-type bits into the helper argument register
            ctx.emitter.instruction("and rsi, 0x7f");                           // isolate the runtime value-type tag for the normalizer
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
            abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
            release_consumed_mixed_to_array_source(ctx);
            ctx.emitter.instruction(&format!("jmp {}", done_label));           // return the retained indexed-array payload
            ctx.emitter.label(&hash_label);
            abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
            abi::emit_incref_if_refcounted(
                ctx.emitter,
                &PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed),
                    value: Box::new(PhpType::Mixed),
                },
            );
            release_consumed_mixed_to_array_source(ctx);
            ctx.emitter.instruction(&format!("jmp {}", done_label));           // return the retained associative payload
        }
    }
    ctx.emitter.label(&invalid_label);
    emit_mixed_to_array_return_type_error(ctx);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Releases a consumed Mixed source while preserving the retained raw array/hash result.
fn release_consumed_mixed_to_array_source(ctx: &mut FunctionContext<'_>) {
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    abi::emit_pop_reg(ctx.emitter, result);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Releases an invalid serializer Mixed owner and raises php-src's concrete return TypeError.
///
/// The mixed-unbox result still carries the exact runtime tag and (for objects) class-id at this
/// point. Dispatch before releasing the cell, then let each non-returning branch consume the
/// saved owner so no exception path can strand it outside frame-local cleanup.
fn emit_mixed_to_array_return_type_error(ctx: &mut FunctionContext<'_>) {
    let int_label = ctx.next_label("date_serialize_return_type_int");
    let string_label = ctx.next_label("date_serialize_return_type_string");
    let float_label = ctx.next_label("date_serialize_return_type_float");
    let bool_label = ctx.next_label("date_serialize_return_type_bool");
    let true_label = ctx.next_label("date_serialize_return_type_true");
    let false_label = ctx.next_label("date_serialize_return_type_false");
    let null_label = ctx.next_label("date_serialize_return_type_null");
    let resource_label = ctx.next_label("date_serialize_return_type_resource");
    let callable_label = ctx.next_label("date_serialize_return_type_callable");
    let object_label = ctx.next_label("date_serialize_return_type_object");
    let incomplete_object_label = ctx.next_label("date_serialize_return_type_incomplete_object");
    let fallback_label = ctx.next_label("date_serialize_return_type_unknown");
    let scalar_cases = [
        ("int", &int_label),
        ("string", &string_label),
        ("float", &float_label),
        ("null", &null_label),
        ("resource", &resource_label),
        ("Closure", &callable_label),
    ];
    let mut object_cases = ctx
        .module
        .class_infos
        .iter()
        .map(|(class_name, info)| {
            (
                info.class_id,
                class_name.trim_start_matches('\\').to_string(),
                ctx.next_label("date_serialize_return_type_object_class"),
            )
        })
        .collect::<Vec<_>>();
    object_cases.sort_by_key(|(class_id, _, _)| *class_id);

    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // classify an invalid serializer return as an integer
            ctx.emitter.instruction(&format!("b.eq {}", int_label));           // select PHP's int-return TypeError wording
            ctx.emitter.instruction("cmp x0, #1");                              // classify an invalid serializer return as a string
            ctx.emitter.instruction(&format!("b.eq {}", string_label));        // select PHP's string-return TypeError wording
            ctx.emitter.instruction("cmp x0, #2");                              // classify an invalid serializer return as a float
            ctx.emitter.instruction(&format!("b.eq {}", float_label));         // select PHP's float-return TypeError wording
            ctx.emitter.instruction("cmp x0, #3");                              // classify an invalid serializer return as a boolean
            ctx.emitter.instruction(&format!("b.eq {}", bool_label));          // select PHP's bool-return TypeError wording
            ctx.emitter.instruction("cmp x0, #6");                              // classify an invalid serializer return as an object
            ctx.emitter.instruction(&format!("b.eq {}", object_label));        // refine the object diagnostic by runtime class id
            ctx.emitter.instruction("cmp x0, #8");                              // classify an invalid serializer return as null
            ctx.emitter.instruction(&format!("b.eq {}", null_label));          // select PHP's null-return TypeError wording
            ctx.emitter.instruction("cmp x0, #9");                              // classify an invalid serializer return as a resource
            ctx.emitter.instruction(&format!("b.eq {}", resource_label));      // select PHP's resource-return TypeError wording
            ctx.emitter.instruction("cmp x0, #10");                             // classify an invalid serializer return as a Closure
            ctx.emitter.instruction(&format!("b.eq {}", callable_label));      // select PHP's Closure-return TypeError wording
            abi::emit_jump(ctx.emitter, &fallback_label);
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // classify an invalid serializer return as an integer
            ctx.emitter.instruction(&format!("je {}", int_label));             // select PHP's int-return TypeError wording
            ctx.emitter.instruction("cmp rax, 1");                              // classify an invalid serializer return as a string
            ctx.emitter.instruction(&format!("je {}", string_label));          // select PHP's string-return TypeError wording
            ctx.emitter.instruction("cmp rax, 2");                              // classify an invalid serializer return as a float
            ctx.emitter.instruction(&format!("je {}", float_label));           // select PHP's float-return TypeError wording
            ctx.emitter.instruction("cmp rax, 3");                              // classify an invalid serializer return as a boolean
            ctx.emitter.instruction(&format!("je {}", bool_label));            // select PHP's bool-return TypeError wording
            ctx.emitter.instruction("cmp rax, 6");                              // classify an invalid serializer return as an object
            ctx.emitter.instruction(&format!("je {}", object_label));          // refine the object diagnostic by runtime class id
            ctx.emitter.instruction("cmp rax, 8");                              // classify an invalid serializer return as null
            ctx.emitter.instruction(&format!("je {}", null_label));            // select PHP's null-return TypeError wording
            ctx.emitter.instruction("cmp rax, 9");                              // classify an invalid serializer return as a resource
            ctx.emitter.instruction(&format!("je {}", resource_label));        // select PHP's resource-return TypeError wording
            ctx.emitter.instruction("cmp rax, 10");                             // classify an invalid serializer return as a Closure
            ctx.emitter.instruction(&format!("je {}", callable_label));        // select PHP's Closure-return TypeError wording
            abi::emit_jump(ctx.emitter, &fallback_label);
        }
    }

    ctx.emitter.label(&object_label);
    let class_id_reg = abi::secondary_scratch_reg(ctx.emitter);
    let candidate_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let payload_reg = match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "x1",
        crate::codegen::platform::Arch::X86_64 => "rdi",
    };
    abi::emit_load_from_address(ctx.emitter, class_id_reg, payload_reg, 0);
    abi::emit_load_int_immediate(ctx.emitter, candidate_reg, -2);
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, candidate_reg)); // identify the reserved __PHP_Incomplete_Class object id before compiled-class matching
            ctx.emitter.instruction(&format!("b.eq {}", incomplete_object_label)); // preserve php-src's concrete incomplete-object return TypeError
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, candidate_reg)); // identify the reserved __PHP_Incomplete_Class object id before compiled-class matching
            ctx.emitter.instruction(&format!("je {}", incomplete_object_label)); // preserve php-src's concrete incomplete-object return TypeError
        }
    }
    for (class_id, _, label) in &object_cases {
        abi::emit_load_int_immediate(ctx.emitter, candidate_reg, *class_id as i64);
        match ctx.emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, candidate_reg)); // compare the returned object's concrete class id
                ctx.emitter.instruction(&format!("b.eq {}", label));           // select this object's exact PHP return TypeError
            }
            crate::codegen::platform::Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, candidate_reg)); // compare the returned object's concrete class id
                ctx.emitter.instruction(&format!("je {}", label));             // select this object's exact PHP return TypeError
            }
        }
    }
    abi::emit_jump(ctx.emitter, &fallback_label);

    ctx.emitter.label(&bool_label);
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));      // PHP names a false return distinctly from true in TypeError messages
            abi::emit_jump(ctx.emitter, &true_label);
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.emitter.instruction("test rdi, rdi");                           // inspect the unboxed boolean payload before formatting TypeError
            ctx.emitter.instruction(&format!("jz {}", false_label));           // zero payload is PHP false
            abi::emit_jump(ctx.emitter, &true_label);
        }
    }

    for (given_type, label) in scalar_cases {
        ctx.emitter.label(label);
        emit_date_serialize_return_type_error_case(ctx, given_type);
    }
    for (_, class_name, label) in object_cases {
        ctx.emitter.label(&label);
        emit_date_serialize_return_type_error_case(ctx, &class_name);
    }
    ctx.emitter.label(&incomplete_object_label);
    emit_date_serialize_return_type_error_case(ctx, "__PHP_Incomplete_Class");
    ctx.emitter.label(&true_label);
    emit_date_serialize_return_type_error_case(ctx, "true");
    ctx.emitter.label(&false_label);
    emit_date_serialize_return_type_error_case(ctx, "false");
    ctx.emitter.label(&fallback_label);
    emit_date_serialize_return_type_error_case(ctx, "object");
}

/// Consumes the saved source cell before producing one exact static return-TypeError message.
fn emit_date_serialize_return_type_error_case(ctx: &mut FunctionContext<'_>, given_type: &str) {
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    let message = format!(
        "{}(): Return value must be of type array, {} returned",
        ctx.function.name, given_type
    );
    super::exceptions::emit_type_error(ctx, &message);
}

/// Finalizes an ambiguously typed DateTime `__serialize()` result as an owned boxed Mixed value.
///
/// The raw return remains an `array` at the ABI boundary so a method call does not pre-emptively
/// stamp an associative DateTime payload as indexed storage. The concrete receiver is passed to
/// the existing date handler guard, which is a no-op for user overrides; the returned container is
/// then boxed from its actual heap kind and its former raw owner is released exactly once.
fn lower_date_serialize_finalize(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime datetime.serialize_finalize expected 2 operands, got {}",
            inst.operands.len(),
        )));
    }
    let raw_result = expect_operand(inst, 0)?;
    let receiver = expect_operand(inst, 1)?;
    let raw_ty = ctx.value_php_type(raw_result)?.codegen_repr();
    let receiver_ty = ctx.value_php_type(receiver)?.codegen_repr();
    if !matches!(&raw_ty, PhpType::Array(value) if value.codegen_repr() == PhpType::Mixed)
        && !matches!(&raw_ty, PhpType::AssocArray { key, value }
            if key.codegen_repr() == PhpType::Str && value.codegen_repr() == PhpType::Mixed)
        || !matches!(&receiver_ty, PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_))
        || inst.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime datetime.serialize_finalize requires array<mixed>, object -> mixed, got {:?}, {:?} -> {:?}",
            raw_ty, receiver_ty, inst.result_php_type
        )));
    }

    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            ctx.load_value_to_reg(raw_result, "x0")?;
            ctx.load_value_to_reg(receiver, "x1")?;
            if !matches!(&receiver_ty, PhpType::Object(_)) {
                ctx.emitter.instruction("ldr x1, [x1, #8]");                   // unbox the dynamically proven object receiver from its Mixed payload word
            }
            abi::emit_call_label(ctx.emitter, "__rt_date_serialize_finalize_mixed");
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.load_value_to_reg(raw_result, "rdi")?;
            ctx.load_value_to_reg(receiver, "rsi")?;
            if !matches!(&receiver_ty, PhpType::Object(_)) {
                ctx.emitter.instruction("mov rsi, QWORD PTR [rsi + 8]");       // unbox the dynamically proven object receiver from its Mixed payload word
            }
            abi::emit_call_label(ctx.emitter, "__rt_date_serialize_finalize_mixed");
        }
    }
    store_if_result(ctx, inst)
}

/// Clones a stored Mixed cell before a nested mutation publishes a new payload.
///
/// A shallow COW clone of an array or hash keeps its boxed Mixed slots shared. This operation
/// preserves scalar tags exactly and delegates tag-4/tag-5 payload retention to
/// `__rt_mixed_from_value`, so the returned cell may be safely promoted and installed only in the
/// mutating parent. A null cell remains null for the caller's existing TypeError guard.
fn lower_mixed_cell_clone(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime array.mixed_cell_clone expected 1 operand, got {}",
            inst.operands.len(),
        )));
    }
    let cell = expect_operand(inst, 0)?;
    let actual = ctx.load_value_to_result(cell)?.codegen_repr();
    if actual != PhpType::Mixed {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime array.mixed_cell_clone expected Mixed, got {:?}",
            actual,
        )));
    }
    let done = ctx.next_label("mixed_cell_clone_done");
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", done));              // absent cells stay absent so the following promotion raises the normal TypeError
            ctx.emitter.instruction("ldr x2, [x0, #16]");                       // load the copied Mixed high payload before reusing x0 for the tag
            ctx.emitter.instruction("ldr x1, [x0, #8]");                        // load the copied Mixed low payload for the retaining box helper
            ctx.emitter.instruction("ldr x0, [x0]");                            // pass the original runtime tag to the retaining box helper
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // absent cells stay absent so the following promotion raises the normal TypeError
            ctx.emitter.instruction(&format!("jz {}", done));                   // bypass payload loads when no boxed cell was stored
            ctx.emitter.instruction("mov rsi, QWORD PTR [rax + 16]");           // load the copied Mixed high payload before reusing rax for the tag
            ctx.emitter.instruction("mov rdi, QWORD PTR [rax + 8]");            // load the copied Mixed low payload for the retaining box helper
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // pass the original runtime tag to the retaining box helper
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Promotes or borrows the array payload of a boxed Mixed cell for a nested key sort.
///
/// The helper mutates tag-4 cells in place, borrows tag-5 payloads unchanged, and returns zero
/// for a null/scalar/missing cell. Its valid hash result remains borrowed from the cell, so EIR
/// ownership stays with the parent storage rather than treating it as freshly owned.
fn lower_mixed_cell_promote_to_hash(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    sort: crate::ir::ArrayKeySort,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime array.mixed_cell_promote_to_hash expected 1 operand, got {}",
            inst.operands.len(),
        )));
    }
    let cell = expect_operand(inst, 0)?;
    let actual = ctx.load_value_to_result(cell)?.codegen_repr();
    if actual != PhpType::Mixed {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime array.mixed_cell_promote_to_hash expected Mixed, got {:?}",
            actual,
        )));
    }
    if ctx.emitter.target.arch == crate::codegen::platform::Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the boxed Mixed cell in the SysV first-argument register
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_cell_promote_to_hash");
    let valid = ctx.next_label("mixed_cell_promote_to_hash_valid");
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", valid));            // nonzero helper results are valid borrowed hash payloads
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // distinguish an invalid Mixed receiver from a hash payload
            ctx.emitter.instruction(&format!("jnz {}", valid));                 // nonzero helper results are valid borrowed hash payloads
        }
    }
    super::exceptions::emit_type_error(
        ctx,
        &format!(
            "{}(): Argument #1 ($array) must be of type array, non-array value given",
            sort.php_name()
        ),
    );
    ctx.emitter.label(&valid);
    store_if_result(ctx, inst)
}

/// Lowers a typed `Str -> Str` transform using the internal string result register pair.
fn lower_unary_string(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    runtime: UnaryStringRuntime,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime {} expected 1 operand, got {}",
            runtime.as_eir(),
            inst.operands.len(),
        )));
    }
    let value = expect_operand(inst, 0)?;
    let actual = ctx.load_value_to_result(value)?.codegen_repr();
    if actual != PhpType::Str {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime {} expected Str, got {:?}",
            runtime.as_eir(),
            actual,
        )));
    }
    abi::emit_call_label(ctx.emitter, unary_string_symbol(runtime));
    store_if_result(ctx, inst)
}

/// Maps a backend-neutral unary string operation to its concrete runtime symbol.
fn unary_string_symbol(runtime: UnaryStringRuntime) -> &'static str {
    match runtime {
        UnaryStringRuntime::AddSlashes => "__rt_addslashes",
        UnaryStringRuntime::Base64Encode => "__rt_base64_encode",
        UnaryStringRuntime::BinToHex => "__rt_bin2hex",
        UnaryStringRuntime::HexToBin => "__rt_hex2bin",
        UnaryStringRuntime::HtmlEntityDecode => "__rt_html_entity_decode",
        UnaryStringRuntime::NlToBr => "__rt_nl2br",
        UnaryStringRuntime::QuoteMeta => "__rt_quotemeta",
        UnaryStringRuntime::QuotedPrintableEncode => "__rt_quoted_printable_encode",
        UnaryStringRuntime::RawUrlDecode => "__rt_urldecode",
        UnaryStringRuntime::RawUrlEncode => "__rt_rawurlencode",
        UnaryStringRuntime::StripSlashes => "__rt_stripslashes",
        UnaryStringRuntime::StrReverse => "__rt_strrev",
        UnaryStringRuntime::StrToLower => "__rt_strtolower",
        UnaryStringRuntime::StrToUpper => "__rt_strtoupper",
        UnaryStringRuntime::UrlDecode => "__rt_urldecode",
        UnaryStringRuntime::UrlEncode => "__rt_urlencode",
    }
}
