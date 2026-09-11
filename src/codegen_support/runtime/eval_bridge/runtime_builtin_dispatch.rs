//! Purpose:
//! Emits the versioned C-ABI dispatcher for shared boxed-cell builtin helpers.
//!
//! Called from:
//! - The eval bridge runtime facade for every supported target.
//!
//! Key details:
//! - Input argument cells are borrowed; successful result cells transfer ownership
//!   through `result_out` to Magician.
//! - Unknown IDs and unsupported arities fail closed without invoking a helper.

use elephc_builtin_contract::{RuntimeBuiltinId, RuntimeBuiltinStatus};

use super::*;

/// Emits the AArch64 implementation of `__elephc_runtime_builtin_call_v1`.
pub(super) fn emit_aarch64_runtime_builtin_dispatch(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_runtime_builtin_call_v1");
    emitter.instruction("sub sp, sp, #64");                                     // allocate an ABI-aligned dispatcher frame
    emitter.instruction("stp x19, x20, [sp]");                                  // preserve callee-saved ID and argument registers
    emitter.instruction("stp x21, x22, [sp, #16]");                             // preserve count and result-out registers
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a stable dispatcher frame pointer
    emitter.instruction("mov x19, x0");                                         // retain the typed runtime builtin ID
    emitter.instruction("mov x20, x1");                                         // retain the borrowed argument-pointer array
    emitter.instruction("mov x21, x2");                                         // retain the boxed argument count
    emitter.instruction("mov x22, x4");                                         // retain the caller-owned result-out slot
    emitter.instruction("cbz x22, __elephc_runtime_builtin_v1_fatal");          // reject a missing result ownership slot
    emitter.instruction("str xzr, [x22]");                                      // clear result-out before any fallible dispatch
    emitter.instruction("cbz x21, __elephc_runtime_builtin_v1_dispatch");       // zero-arity calls need no argument array
    emitter.instruction("cbz x20, __elephc_runtime_builtin_v1_fatal");          // non-empty calls require a readable pointer array
    emitter.label("__elephc_runtime_builtin_v1_dispatch");

    for (id, label) in runtime_dispatch_labels() {
        emitter.instruction(&format!("cmp x19, #{}", id.as_u32()));             // compare the retained builtin ID with this arm's contract value
        emitter.instruction(&format!("b.eq {label}"));                          // dispatch to the matching builtin arm
    }
    emitter.instruction("b __elephc_runtime_builtin_v1_unsupported");           // unknown IDs fail closed

    emit_aarch64_unary_case(emitter, RuntimeBuiltinId::Boolval, "boolval", "__elephc_eval_value_cast_bool");
    emit_aarch64_unary_case(emitter, RuntimeBuiltinId::Floatval, "floatval", "__elephc_eval_value_cast_float");
    emit_aarch64_unary_case(emitter, RuntimeBuiltinId::Intval, "intval", "__elephc_eval_value_cast_int");
    emit_aarch64_is_array_case(emitter);
    emit_aarch64_bool_predicate_case(emitter, RuntimeBuiltinId::IsNull, "is_null", "__elephc_eval_value_is_null");
    emit_aarch64_unary_case(emitter, RuntimeBuiltinId::Abs, "abs", "__elephc_eval_value_abs");
    emit_aarch64_unary_case(emitter, RuntimeBuiltinId::Ceil, "ceil", "__elephc_eval_value_ceil");
    emit_aarch64_unary_case(emitter, RuntimeBuiltinId::Floor, "floor", "__elephc_eval_value_floor");
    emit_aarch64_unary_case(emitter, RuntimeBuiltinId::Sqrt, "sqrt", "__elephc_eval_value_sqrt");
    emit_aarch64_binary_case(emitter, RuntimeBuiltinId::Fdiv, "fdiv", "__elephc_eval_value_fdiv");
    emit_aarch64_binary_case(emitter, RuntimeBuiltinId::Fmod, "fmod", "__elephc_eval_value_fmod");
    emit_aarch64_binary_case(emitter, RuntimeBuiltinId::Pow, "pow", "__elephc_eval_value_pow");
    emit_aarch64_round_case(emitter);
    emit_aarch64_unary_case(emitter, RuntimeBuiltinId::Strrev, "strrev", "__elephc_eval_value_strrev");
    emit_aarch64_binary_case(emitter, RuntimeBuiltinId::ArrayKeyExists, "array_key_exists", "__elephc_eval_value_array_key_exists");
    emit_aarch64_zero_arg_boxed_int_case(emitter, RuntimeBuiltinId::ObGetLevel, "ob_get_level", "__elephc_eval_ob_level");
    emit_aarch64_ob_length_case(emitter);
    emit_aarch64_zero_arg_boxed_bool_case(emitter, RuntimeBuiltinId::ObClean, "ob_clean", "__elephc_eval_ob_clean", None);
    emit_aarch64_zero_arg_boxed_bool_case(emitter, RuntimeBuiltinId::ObFlush, "ob_flush", "__elephc_eval_ob_flush", None);
    emit_aarch64_zero_arg_boxed_bool_case(emitter, RuntimeBuiltinId::ObEndClean, "ob_end_clean", "__elephc_eval_ob_end", Some(0));
    emit_aarch64_zero_arg_boxed_bool_case(emitter, RuntimeBuiltinId::ObEndFlush, "ob_end_flush", "__elephc_eval_ob_end", Some(1));

    emitter.label("__elephc_runtime_builtin_v1_result");
    emitter.instruction("cbz x0, __elephc_runtime_builtin_v1_fatal");           // null helper results report runtime failure
    emitter.instruction("str x0, [x22]");                                       // transfer the fresh boxed result to the caller
    emitter.instruction(&format!("mov w0, #{}", RuntimeBuiltinStatus::Success as i32)); // report Success through the versioned status contract
    emitter.instruction("b __elephc_runtime_builtin_v1_done");                  // return success after ownership transfer
    emitter.label("__elephc_runtime_builtin_v1_fatal");
    emitter.instruction(&format!("mov w0, #{}", RuntimeBuiltinStatus::RuntimeFatal as i32)); // report RuntimeFatal; result-out stays null
    emitter.instruction("b __elephc_runtime_builtin_v1_done");                  // return failure with a null result slot
    emitter.label("__elephc_runtime_builtin_v1_unsupported");
    emitter.instruction(&format!("mov w0, #{}", RuntimeBuiltinStatus::Unsupported as i32)); // report Unsupported for unknown IDs and arities
    emitter.label("__elephc_runtime_builtin_v1_done");
    emitter.instruction("ldp x19, x20, [sp]");                                  // restore callee-saved ID and arguments
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore callee-saved count and result slot
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the dispatcher frame
    emitter.instruction("ret");                                                 // return the version-one runtime status
}

/// Emits the x86_64 implementation of `__elephc_runtime_builtin_call_v1`.
pub(super) fn emit_x86_64_runtime_builtin_dispatch(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_runtime_builtin_call_v1");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable dispatcher frame
    emitter.instruction("push rbx");                                            // preserve the typed runtime builtin ID
    emitter.instruction("push r12");                                            // preserve the borrowed argument-pointer array
    emitter.instruction("push r13");                                            // preserve the boxed argument count
    emitter.instruction("push r15");                                            // preserve the caller-owned result-out slot and call alignment
    emitter.instruction("mov ebx, edi");                                        // retain the typed runtime builtin ID
    emitter.instruction("mov r12, rsi");                                        // retain the borrowed argument-pointer array
    emitter.instruction("mov r13, rdx");                                        // retain the boxed argument count
    emitter.instruction("mov r15, r8");                                         // retain the caller-owned result-out slot
    emitter.instruction("test r15, r15");                                       // validate the result ownership slot
    emitter.instruction("jz __elephc_runtime_builtin_v1_fatal_x86");            // reject a missing result-out pointer
    emitter.instruction("mov QWORD PTR [r15], 0");                              // clear result-out before fallible dispatch
    emitter.instruction("test r13, r13");                                       // zero-arity calls need no argument array
    emitter.instruction("jz __elephc_runtime_builtin_v1_dispatch_x86");         // proceed directly for zero-argument helpers
    emitter.instruction("test r12, r12");                                       // non-empty calls require a readable pointer array
    emitter.instruction("jz __elephc_runtime_builtin_v1_fatal_x86");            // fail closed for a missing argument array
    emitter.label("__elephc_runtime_builtin_v1_dispatch_x86");

    for (id, label) in runtime_dispatch_labels() {
        emitter.instruction(&format!("cmp ebx, {}", id.as_u32()));              // compare the retained builtin ID with this arm's contract value
        emitter.instruction(&format!("je {label}_x86"));                        // dispatch to the matching builtin arm
    }
    emitter.instruction("jmp __elephc_runtime_builtin_v1_unsupported_x86");     // unknown IDs fail closed

    emit_x86_64_unary_case(emitter, RuntimeBuiltinId::Boolval, "boolval", "__elephc_eval_value_cast_bool");
    emit_x86_64_unary_case(emitter, RuntimeBuiltinId::Floatval, "floatval", "__elephc_eval_value_cast_float");
    emit_x86_64_unary_case(emitter, RuntimeBuiltinId::Intval, "intval", "__elephc_eval_value_cast_int");
    emit_x86_64_is_array_case(emitter);
    emit_x86_64_bool_predicate_case(emitter, RuntimeBuiltinId::IsNull, "is_null", "__elephc_eval_value_is_null");
    emit_x86_64_unary_case(emitter, RuntimeBuiltinId::Abs, "abs", "__elephc_eval_value_abs");
    emit_x86_64_unary_case(emitter, RuntimeBuiltinId::Ceil, "ceil", "__elephc_eval_value_ceil");
    emit_x86_64_unary_case(emitter, RuntimeBuiltinId::Floor, "floor", "__elephc_eval_value_floor");
    emit_x86_64_unary_case(emitter, RuntimeBuiltinId::Sqrt, "sqrt", "__elephc_eval_value_sqrt");
    emit_x86_64_binary_case(emitter, RuntimeBuiltinId::Fdiv, "fdiv", "__elephc_eval_value_fdiv");
    emit_x86_64_binary_case(emitter, RuntimeBuiltinId::Fmod, "fmod", "__elephc_eval_value_fmod");
    emit_x86_64_binary_case(emitter, RuntimeBuiltinId::Pow, "pow", "__elephc_eval_value_pow");
    emit_x86_64_round_case(emitter);
    emit_x86_64_unary_case(emitter, RuntimeBuiltinId::Strrev, "strrev", "__elephc_eval_value_strrev");
    emit_x86_64_binary_case(emitter, RuntimeBuiltinId::ArrayKeyExists, "array_key_exists", "__elephc_eval_value_array_key_exists");
    emit_x86_64_zero_arg_boxed_int_case(emitter, RuntimeBuiltinId::ObGetLevel, "ob_get_level", "__elephc_eval_ob_level");
    emit_x86_64_ob_length_case(emitter);
    emit_x86_64_zero_arg_boxed_bool_case(emitter, RuntimeBuiltinId::ObClean, "ob_clean", "__elephc_eval_ob_clean", None);
    emit_x86_64_zero_arg_boxed_bool_case(emitter, RuntimeBuiltinId::ObFlush, "ob_flush", "__elephc_eval_ob_flush", None);
    emit_x86_64_zero_arg_boxed_bool_case(emitter, RuntimeBuiltinId::ObEndClean, "ob_end_clean", "__elephc_eval_ob_end", Some(0));
    emit_x86_64_zero_arg_boxed_bool_case(emitter, RuntimeBuiltinId::ObEndFlush, "ob_end_flush", "__elephc_eval_ob_end", Some(1));

    emitter.label("__elephc_runtime_builtin_v1_result_x86");
    emitter.instruction("test rax, rax");                                       // null helper results report runtime failure
    emitter.instruction("jz __elephc_runtime_builtin_v1_fatal_x86");            // keep result-out null on failure
    emitter.instruction("mov QWORD PTR [r15], rax");                            // transfer the fresh boxed result to the caller
    emitter.instruction(&format!("mov eax, {}", RuntimeBuiltinStatus::Success as i32)); // report Success through the versioned status contract
    emitter.instruction("jmp __elephc_runtime_builtin_v1_done_x86");            // return success after ownership transfer
    emitter.label("__elephc_runtime_builtin_v1_fatal_x86");
    emitter.instruction(&format!("mov eax, {}", RuntimeBuiltinStatus::RuntimeFatal as i32)); // report RuntimeFatal; result-out stays null
    emitter.instruction("jmp __elephc_runtime_builtin_v1_done_x86");            // return failure with a null result slot
    emitter.label("__elephc_runtime_builtin_v1_unsupported_x86");
    emitter.instruction(&format!("mov eax, {}", RuntimeBuiltinStatus::Unsupported as i32)); // report Unsupported for unknown IDs and arities
    emitter.label("__elephc_runtime_builtin_v1_done_x86");
    emitter.instruction("pop r15");                                             // restore the caller-owned result-out register
    emitter.instruction("pop r13");                                             // restore the boxed argument count register
    emitter.instruction("pop r12");                                             // restore the borrowed argument-array register
    emitter.instruction("pop rbx");                                             // restore the typed runtime builtin ID register
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the version-one runtime status
}

/// Returns stable branch labels for every version-one runtime builtin ID.
fn runtime_dispatch_labels() -> [(RuntimeBuiltinId, &'static str); 21] {
    let labels = [
        (RuntimeBuiltinId::Boolval, "__elephc_runtime_builtin_v1_boolval"),
        (RuntimeBuiltinId::Floatval, "__elephc_runtime_builtin_v1_floatval"),
        (RuntimeBuiltinId::Intval, "__elephc_runtime_builtin_v1_intval"),
        (RuntimeBuiltinId::IsArray, "__elephc_runtime_builtin_v1_is_array"),
        (RuntimeBuiltinId::IsNull, "__elephc_runtime_builtin_v1_is_null"),
        (RuntimeBuiltinId::Abs, "__elephc_runtime_builtin_v1_abs"),
        (RuntimeBuiltinId::Ceil, "__elephc_runtime_builtin_v1_ceil"),
        (RuntimeBuiltinId::Floor, "__elephc_runtime_builtin_v1_floor"),
        (RuntimeBuiltinId::Sqrt, "__elephc_runtime_builtin_v1_sqrt"),
        (RuntimeBuiltinId::Fdiv, "__elephc_runtime_builtin_v1_fdiv"),
        (RuntimeBuiltinId::Fmod, "__elephc_runtime_builtin_v1_fmod"),
        (RuntimeBuiltinId::Pow, "__elephc_runtime_builtin_v1_pow"),
        (RuntimeBuiltinId::Round, "__elephc_runtime_builtin_v1_round"),
        (RuntimeBuiltinId::Strrev, "__elephc_runtime_builtin_v1_strrev"),
        (RuntimeBuiltinId::ArrayKeyExists, "__elephc_runtime_builtin_v1_array_key_exists"),
        (RuntimeBuiltinId::ObGetLevel, "__elephc_runtime_builtin_v1_ob_get_level"),
        (RuntimeBuiltinId::ObGetLength, "__elephc_runtime_builtin_v1_ob_get_length"),
        (RuntimeBuiltinId::ObClean, "__elephc_runtime_builtin_v1_ob_clean"),
        (RuntimeBuiltinId::ObFlush, "__elephc_runtime_builtin_v1_ob_flush"),
        (RuntimeBuiltinId::ObEndClean, "__elephc_runtime_builtin_v1_ob_end_clean"),
        (RuntimeBuiltinId::ObEndFlush, "__elephc_runtime_builtin_v1_ob_end_flush"),
    ];
    for (runtime_id, _) in labels {
        let binding = crate::builtins::registry::lookup_runtime_builtin(runtime_id);
        assert_eq!(
            binding.spec.id(),
            runtime_id.builtin_id(),
            "boxed runtime ID must join to its canonical AOT contract"
        );
    }
    labels
}

/// Emits one AArch64 unary boxed-result dispatch arm.
fn emit_aarch64_unary_case(emitter: &mut Emitter, _id: RuntimeBuiltinId, label: &str, symbol: &str) {
    emitter.label(&format!("__elephc_runtime_builtin_v1_{label}"));
    emitter.instruction("cmp x21, #1");                                         // require one boxed argument
    emitter.instruction("b.ne __elephc_runtime_builtin_v1_unsupported");        // preserve fallback for unsupported arity
    emitter.instruction("ldr x0, [x20]");                                       // load the borrowed boxed argument
    emitter.bl_c(symbol);
    emitter.instruction("b __elephc_runtime_builtin_v1_result");                // validate and transfer the boxed result
}

/// Emits one AArch64 binary boxed-result dispatch arm.
fn emit_aarch64_binary_case(emitter: &mut Emitter, _id: RuntimeBuiltinId, label: &str, symbol: &str) {
    emitter.label(&format!("__elephc_runtime_builtin_v1_{label}"));
    emitter.instruction("cmp x21, #2");                                         // require two boxed arguments
    emitter.instruction("b.ne __elephc_runtime_builtin_v1_unsupported");        // preserve fallback for unsupported arity
    emitter.instruction("ldp x0, x1, [x20]");                                   // load borrowed boxed arguments in PHP order
    emitter.bl_c(symbol);
    emitter.instruction("b __elephc_runtime_builtin_v1_result");                // validate and transfer the boxed result
}

/// Emits the AArch64 `is_array` tag predicate arm.
fn emit_aarch64_is_array_case(emitter: &mut Emitter) {
    emitter.label("__elephc_runtime_builtin_v1_is_array");
    emitter.instruction("cmp x21, #1");                                         // require one boxed argument
    emitter.instruction("b.ne __elephc_runtime_builtin_v1_unsupported");        // reject unsupported arity
    emitter.instruction("ldr x0, [x20]");                                       // load the borrowed boxed value
    emitter.bl_c("__elephc_eval_value_type_tag");
    emitter.instruction("cmp x0, #4");                                          // tag 4 is an indexed PHP array
    emitter.instruction("b.eq __elephc_runtime_builtin_v1_is_array_true");      // accept indexed arrays
    emitter.instruction("cmp x0, #5");                                          // tag 5 is an associative PHP array
    emitter.instruction("cset x0, eq");                                         // return false unless the tag is associative
    emitter.instruction("b __elephc_runtime_builtin_v1_is_array_box");          // box the predicate result
    emitter.label("__elephc_runtime_builtin_v1_is_array_true");
    emitter.instruction("mov x0, #1");                                          // report true for indexed arrays
    emitter.label("__elephc_runtime_builtin_v1_is_array_box");
    emitter.bl_c("__elephc_eval_value_bool");
    emitter.instruction("b __elephc_runtime_builtin_v1_result");                // transfer the boxed boolean result
}

/// Emits one AArch64 raw-boolean predicate arm and boxes its result.
fn emit_aarch64_bool_predicate_case(emitter: &mut Emitter, _id: RuntimeBuiltinId, label: &str, symbol: &str) {
    emitter.label(&format!("__elephc_runtime_builtin_v1_{label}"));
    emitter.instruction("cmp x21, #1");                                         // require one boxed argument
    emitter.instruction("b.ne __elephc_runtime_builtin_v1_unsupported");        // reject unsupported arity
    emitter.instruction("ldr x0, [x20]");                                       // load the borrowed boxed value
    emitter.bl_c(symbol);
    emitter.bl_c("__elephc_eval_value_bool");
    emitter.instruction("b __elephc_runtime_builtin_v1_result");                // transfer the boxed boolean result
}

/// Emits the AArch64 optional-precision `round` dispatch arm.
fn emit_aarch64_round_case(emitter: &mut Emitter) {
    emitter.label("__elephc_runtime_builtin_v1_round");
    emitter.instruction("cmp x21, #1");                                         // select the default-precision form
    emitter.instruction("b.eq __elephc_runtime_builtin_v1_round_one");          // omit the precision cell
    emitter.instruction("cmp x21, #2");                                         // otherwise require an explicit precision cell
    emitter.instruction("b.ne __elephc_runtime_builtin_v1_unsupported");        // reject unsupported arity
    emitter.instruction("ldp x0, x1, [x20]");                                   // load value and precision cells
    emitter.instruction("mov x2, #1");                                          // mark the precision cell present
    emitter.instruction("b __elephc_runtime_builtin_v1_round_call");            // share the runtime wrapper call
    emitter.label("__elephc_runtime_builtin_v1_round_one");
    emitter.instruction("ldr x0, [x20]");                                       // load the value cell
    emitter.instruction("mov x1, xzr");                                         // no precision pointer is present
    emitter.instruction("mov x2, xzr");                                         // mark the optional precision absent
    emitter.label("__elephc_runtime_builtin_v1_round_call");
    emitter.bl_c("__elephc_eval_value_round");
    emitter.instruction("b __elephc_runtime_builtin_v1_result");                // transfer the boxed numeric result
}

/// Emits one AArch64 zero-argument integer result arm.
fn emit_aarch64_zero_arg_boxed_int_case(emitter: &mut Emitter, _id: RuntimeBuiltinId, label: &str, symbol: &str) {
    emitter.label(&format!("__elephc_runtime_builtin_v1_{label}"));
    emitter.instruction("cbnz x21, __elephc_runtime_builtin_v1_unsupported");   // require zero PHP arguments
    emitter.bl_c(symbol);
    emitter.bl_c("__elephc_eval_value_int");
    emitter.instruction("b __elephc_runtime_builtin_v1_result");                // transfer the boxed integer result
}

/// Emits the AArch64 length-or-false `ob_get_length` result arm.
fn emit_aarch64_ob_length_case(emitter: &mut Emitter) {
    emitter.label("__elephc_runtime_builtin_v1_ob_get_length");
    emitter.instruction("cbnz x21, __elephc_runtime_builtin_v1_unsupported");   // require zero PHP arguments
    emitter.bl_c("__elephc_eval_ob_length");
    emitter.instruction("tbnz x0, #63, __elephc_runtime_builtin_v1_ob_length_false"); // negative sentinel means no active buffer
    emitter.bl_c("__elephc_eval_value_int");
    emitter.instruction("b __elephc_runtime_builtin_v1_result");                // transfer the boxed length
    emitter.label("__elephc_runtime_builtin_v1_ob_length_false");
    emitter.instruction("mov x0, #0");                                          // materialize raw false for the boolean boxer
    emitter.bl_c("__elephc_eval_value_bool");
    emitter.instruction("b __elephc_runtime_builtin_v1_result");                // transfer boxed PHP false
}

/// Emits one AArch64 zero-argument boolean result arm.
fn emit_aarch64_zero_arg_boxed_bool_case(
    emitter: &mut Emitter,
    _id: RuntimeBuiltinId,
    label: &str,
    symbol: &str,
    flag: Option<u64>,
) {
    emitter.label(&format!("__elephc_runtime_builtin_v1_{label}"));
    emitter.instruction("cbnz x21, __elephc_runtime_builtin_v1_unsupported");   // require zero PHP arguments
    if let Some(flag) = flag {
        emitter.instruction(&format!("mov x0, #{flag}"));                       // select clean or flush end behavior
    }
    emitter.bl_c(symbol);
    emitter.bl_c("__elephc_eval_value_bool");
    emitter.instruction("b __elephc_runtime_builtin_v1_result");                // transfer the boxed boolean result
}

/// Emits one x86_64 unary boxed-result dispatch arm.
fn emit_x86_64_unary_case(emitter: &mut Emitter, _id: RuntimeBuiltinId, label: &str, symbol: &str) {
    emitter.label(&format!("__elephc_runtime_builtin_v1_{label}_x86"));
    emitter.instruction("cmp r13, 1");                                          // require one boxed argument
    emitter.instruction("jne __elephc_runtime_builtin_v1_unsupported_x86");     // preserve fallback for unsupported arity
    emitter.instruction("mov rdi, QWORD PTR [r12]");                            // load the borrowed boxed argument
    emitter.bl_c(symbol);
    emitter.instruction("jmp __elephc_runtime_builtin_v1_result_x86");          // validate and transfer the boxed result
}

/// Emits one x86_64 binary boxed-result dispatch arm.
fn emit_x86_64_binary_case(emitter: &mut Emitter, _id: RuntimeBuiltinId, label: &str, symbol: &str) {
    emitter.label(&format!("__elephc_runtime_builtin_v1_{label}_x86"));
    emitter.instruction("cmp r13, 2");                                          // require two boxed arguments
    emitter.instruction("jne __elephc_runtime_builtin_v1_unsupported_x86");     // preserve fallback for unsupported arity
    emitter.instruction("mov rdi, QWORD PTR [r12]");                            // load the first borrowed boxed argument
    emitter.instruction("mov rsi, QWORD PTR [r12 + 8]");                        // load the second borrowed boxed argument
    emitter.bl_c(symbol);
    emitter.instruction("jmp __elephc_runtime_builtin_v1_result_x86");          // validate and transfer the boxed result
}

/// Emits the x86_64 `is_array` tag predicate arm.
fn emit_x86_64_is_array_case(emitter: &mut Emitter) {
    emitter.label("__elephc_runtime_builtin_v1_is_array_x86");
    emitter.instruction("cmp r13, 1");                                          // require one boxed argument
    emitter.instruction("jne __elephc_runtime_builtin_v1_unsupported_x86");     // reject unsupported arity
    emitter.instruction("mov rdi, QWORD PTR [r12]");                            // load the borrowed boxed value
    emitter.bl_c("__elephc_eval_value_type_tag");
    emitter.instruction("cmp rax, 4");                                          // tag 4 is an indexed PHP array
    emitter.instruction("je __elephc_runtime_builtin_v1_is_array_true_x86");    // accept indexed arrays
    emitter.instruction("cmp rax, 5");                                          // tag 5 is an associative PHP array
    emitter.instruction("sete dil");                                            // set the low result byte for associative arrays
    emitter.instruction("movzx edi, dil");                                      // widen the raw boolean C argument
    emitter.instruction("jmp __elephc_runtime_builtin_v1_is_array_box_x86");    // box the predicate result
    emitter.label("__elephc_runtime_builtin_v1_is_array_true_x86");
    emitter.instruction("mov edi, 1");                                          // report true for indexed arrays
    emitter.label("__elephc_runtime_builtin_v1_is_array_box_x86");
    emitter.bl_c("__elephc_eval_value_bool");
    emitter.instruction("jmp __elephc_runtime_builtin_v1_result_x86");          // transfer the boxed boolean result
}

/// Emits one x86_64 raw-boolean predicate arm and boxes its result.
fn emit_x86_64_bool_predicate_case(emitter: &mut Emitter, _id: RuntimeBuiltinId, label: &str, symbol: &str) {
    emitter.label(&format!("__elephc_runtime_builtin_v1_{label}_x86"));
    emitter.instruction("cmp r13, 1");                                          // require one boxed argument
    emitter.instruction("jne __elephc_runtime_builtin_v1_unsupported_x86");     // reject unsupported arity
    emitter.instruction("mov rdi, QWORD PTR [r12]");                            // load the borrowed boxed value
    emitter.bl_c(symbol);
    emitter.instruction("mov rdi, rax");                                        // pass the raw predicate to the bool boxer
    emitter.bl_c("__elephc_eval_value_bool");
    emitter.instruction("jmp __elephc_runtime_builtin_v1_result_x86");          // transfer the boxed boolean result
}

/// Emits the x86_64 optional-precision `round` dispatch arm.
fn emit_x86_64_round_case(emitter: &mut Emitter) {
    emitter.label("__elephc_runtime_builtin_v1_round_x86");
    emitter.instruction("cmp r13, 1");                                          // select the default-precision form
    emitter.instruction("je __elephc_runtime_builtin_v1_round_one_x86");        // omit the precision cell
    emitter.instruction("cmp r13, 2");                                          // otherwise require an explicit precision cell
    emitter.instruction("jne __elephc_runtime_builtin_v1_unsupported_x86");     // reject unsupported arity
    emitter.instruction("mov rdi, QWORD PTR [r12]");                            // load the value cell
    emitter.instruction("mov rsi, QWORD PTR [r12 + 8]");                        // load the precision cell
    emitter.instruction("mov edx, 1");                                          // mark the precision cell present
    emitter.instruction("jmp __elephc_runtime_builtin_v1_round_call_x86");      // share the runtime wrapper call
    emitter.label("__elephc_runtime_builtin_v1_round_one_x86");
    emitter.instruction("mov rdi, QWORD PTR [r12]");                            // load the value cell
    emitter.instruction("xor esi, esi");                                        // no precision pointer is present
    emitter.instruction("xor edx, edx");                                        // mark the optional precision absent
    emitter.label("__elephc_runtime_builtin_v1_round_call_x86");
    emitter.bl_c("__elephc_eval_value_round");
    emitter.instruction("jmp __elephc_runtime_builtin_v1_result_x86");          // transfer the boxed numeric result
}

/// Emits one x86_64 zero-argument integer result arm.
fn emit_x86_64_zero_arg_boxed_int_case(emitter: &mut Emitter, _id: RuntimeBuiltinId, label: &str, symbol: &str) {
    emitter.label(&format!("__elephc_runtime_builtin_v1_{label}_x86"));
    emitter.instruction("test r13, r13");                                       // require zero PHP arguments
    emitter.instruction("jnz __elephc_runtime_builtin_v1_unsupported_x86");     // reject unsupported arity
    emitter.bl_c(symbol);
    emitter.instruction("mov rdi, rax");                                        // pass the raw integer to the boxer
    emitter.bl_c("__elephc_eval_value_int");
    emitter.instruction("jmp __elephc_runtime_builtin_v1_result_x86");          // transfer the boxed integer result
}

/// Emits the x86_64 length-or-false `ob_get_length` result arm.
fn emit_x86_64_ob_length_case(emitter: &mut Emitter) {
    emitter.label("__elephc_runtime_builtin_v1_ob_get_length_x86");
    emitter.instruction("test r13, r13");                                       // require zero PHP arguments
    emitter.instruction("jnz __elephc_runtime_builtin_v1_unsupported_x86");     // reject unsupported arity
    emitter.bl_c("__elephc_eval_ob_length");
    emitter.instruction("test rax, rax");                                       // negative sentinel means no active buffer
    emitter.instruction("js __elephc_runtime_builtin_v1_ob_length_false_x86");  // select boxed PHP false
    emitter.instruction("mov rdi, rax");                                        // pass the byte length to the integer boxer
    emitter.bl_c("__elephc_eval_value_int");
    emitter.instruction("jmp __elephc_runtime_builtin_v1_result_x86");          // transfer the boxed length
    emitter.label("__elephc_runtime_builtin_v1_ob_length_false_x86");
    emitter.instruction("xor edi, edi");                                        // materialize raw false for the boolean boxer
    emitter.bl_c("__elephc_eval_value_bool");
    emitter.instruction("jmp __elephc_runtime_builtin_v1_result_x86");          // transfer boxed PHP false
}

/// Emits one x86_64 zero-argument boolean result arm.
fn emit_x86_64_zero_arg_boxed_bool_case(
    emitter: &mut Emitter,
    _id: RuntimeBuiltinId,
    label: &str,
    symbol: &str,
    flag: Option<u64>,
) {
    emitter.label(&format!("__elephc_runtime_builtin_v1_{label}_x86"));
    emitter.instruction("test r13, r13");                                       // require zero PHP arguments
    emitter.instruction("jnz __elephc_runtime_builtin_v1_unsupported_x86");     // reject unsupported arity
    if let Some(flag) = flag {
        emitter.instruction(&format!("mov edi, {flag}"));                       // select clean or flush end behavior
    }
    emitter.bl_c(symbol);
    emitter.instruction("mov rdi, rax");                                        // pass the raw result to the bool boxer
    emitter.bl_c("__elephc_eval_value_bool");
    emitter.instruction("jmp __elephc_runtime_builtin_v1_result_x86");          // transfer the boxed boolean result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Emits the version-one runtime dispatcher for one target.
    fn emit_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        match target.arch {
            Arch::AArch64 => emit_aarch64_runtime_builtin_dispatch(&mut emitter),
            Arch::X86_64 => emit_x86_64_runtime_builtin_dispatch(&mut emitter),
        }
        emitter.output()
    }

    /// Verifies both target backends export the versioned typed dispatcher and fail closed.
    #[test]
    fn runtime_builtin_dispatch_is_emitted_for_all_supported_architectures() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = emit_for(target);
            let symbol = target.extern_symbol("__elephc_runtime_builtin_call_v1");
            assert!(asm.contains(&format!("{symbol}:")), "{target:?}:\n{asm}");
            assert!(asm.contains("runtime_builtin_v1_unsupported"), "{target:?}:\n{asm}");
        }
    }

    /// Verifies representative types, math, strings, arrays, and I/O IDs have dispatch arms.
    #[test]
    fn runtime_builtin_dispatch_covers_each_migrated_family() {
        let asm = emit_for(Target::new(Platform::Linux, Arch::X86_64));
        for label in [
            "runtime_builtin_v1_boolval_x86",
            "runtime_builtin_v1_abs_x86",
            "runtime_builtin_v1_strrev_x86",
            "runtime_builtin_v1_array_key_exists_x86",
            "runtime_builtin_v1_ob_get_level_x86",
        ] {
            assert!(asm.contains(label), "missing {label}:\n{asm}");
        }
    }

    /// Verifies the generated runtime preserves PHP's false sentinel for
    /// `ob_get_length()` when no output buffer is active on both architectures.
    #[test]
    fn ob_get_length_boxes_false_without_an_active_buffer() {
        for (target, label, next_label, zero_instruction) in [
            (
                Target::new(Platform::Linux, Arch::AArch64),
                "__elephc_runtime_builtin_v1_ob_length_false:\n",
                "__elephc_runtime_builtin_v1_result:\n",
                "mov x0, #0",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "__elephc_runtime_builtin_v1_ob_length_false_x86:\n",
                "__elephc_runtime_builtin_v1_result_x86:\n",
                "xor edi, edi",
            ),
        ] {
            let asm = emit_for(target);
            let false_arm = asm
                .split(label)
                .nth(1)
                .unwrap_or_else(|| panic!("missing false arm for {target:?}:\n{asm}"))
                .split(next_label)
                .next()
                .expect("false arm precedes the shared result arm");
            let bool_boxer = target.extern_symbol("__elephc_eval_value_bool");
            let null_boxer = target.extern_symbol("__elephc_eval_value_null");
            assert!(
                false_arm.contains(zero_instruction),
                "false arm must materialize zero for {target:?}:\n{false_arm}"
            );
            assert!(
                false_arm.contains(&bool_boxer),
                "false arm must use the boolean boxer for {target:?}:\n{false_arm}"
            );
            assert!(
                !false_arm.contains(&null_boxer),
                "false arm must not use the null boxer for {target:?}:\n{false_arm}"
            );
        }
    }
}
