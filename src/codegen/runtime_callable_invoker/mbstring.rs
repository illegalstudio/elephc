//! Purpose:
//! Forwards callable arguments to the shared mbstring invocation coordinator.
//!
//! Called from:
//! - The descriptor invoker after classifying its normalized argument container.
//!
//! Key details:
//! - Borrowed cells preserve actual arity, omitted defaults, and original PHP types.
//! - The shared coordinator owns conversion and returns a single owned Mixed result.

use super::*;
use elephc_builtin_contract::RuntimeBuiltinId;

/// Emits PHP warnings for ordinary values in an indexed callable argument container.
/// A ref marker or PHP reference supplies live caller storage and needs no warning.
pub(super) fn warn_indexed_nonreference_roots(
    emitter: &mut Emitter, ctx: &mut InvokerEmitContext, data: &mut DataSection,
    array_reg: &str, len_reg: &str, element_stride: usize,
) {
    let cursor = if emitter.target.arch == Arch::AArch64 { "x23" } else { "rbx" };
    let cell = abi::int_result_reg(emitter);
    let tag = abi::secondary_scratch_reg(emitter);
    let loop_label = ctx.next_label("mb_variables_refs_loop");
    let done_label = ctx.next_label("mb_variables_refs_done");
    let accepted_label = ctx.next_label("mb_variables_ref_accepted");
    let warn_label = ctx.next_label("mb_variables_ref_warn");
    abi::emit_load_int_immediate(emitter, cursor, 2);
    emitter.label(&loop_label);
    super::emit_compare_reg_ge(emitter, cursor, len_reg, &done_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x9, #{element_stride}"));         // select the indexed argument's physical stride
            emitter.instruction("mul x9, x23, x9");                             // locate this argument beyond the array header
            emitter.instruction("add x9, x9, #24");                             // skip the indexed array header
            emitter.instruction(&format!("ldr {cell}, [{array_reg}, x9]"));     // load the original Mixed argument cell
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, rbx");                                // start with the current argument index
            emitter.instruction(&format!("imul r10, {element_stride}"));        // locate this argument beyond the array header
            emitter.instruction(&format!("mov {cell}, QWORD PTR [{array_reg} + r10 + 24]")); // load the original Mixed argument cell
        }
    }
    abi::emit_load_from_address(emitter, tag, cell, 0);
    super::emit_branch_if_invoker_ref_cell_tag(tag, &accepted_label, emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {tag}, #11"));                    // a PHP reference owns a writable caller cell
            emitter.instruction(&format!("b.eq {accepted_label}"));             // retain that existing reference
            emitter.instruction(&format!("cmp {tag}, #7"));                     // a persistent wrapper can hold an indexed reference
            emitter.instruction(&format!("b.ne {warn_label}"));                 // ordinary values require PHP's by-reference warning
            abi::emit_load_from_address(emitter, tag, cell, 16);
            emitter.instruction(&format!("cmp {tag}, #1"));                     // identify the persistent reference marker
            emitter.instruction(&format!("b.eq {accepted_label}"));             // preserve its live storage
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {tag}, 11"));                     // a PHP reference owns a writable caller cell
            emitter.instruction(&format!("je {accepted_label}"));               // retain that existing reference
            emitter.instruction(&format!("cmp {tag}, 7"));                      // a persistent wrapper can hold an indexed reference
            emitter.instruction(&format!("jne {warn_label}"));                  // ordinary values require PHP's by-reference warning
            abi::emit_load_from_address(emitter, tag, cell, 16);
            emitter.instruction(&format!("cmp {tag}, 1"));                      // identify the persistent reference marker
            emitter.instruction(&format!("je {accepted_label}"));               // preserve its live storage
        }
    }
    emitter.label(&warn_label);
    emit_missing_reference_warning(emitter, ctx, data, cursor);
    emitter.label(&accepted_label);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("add x23, x23, #1"),               // advance to the next supplied variable
        Arch::X86_64 => emitter.instruction("add rbx, 1"),                      // advance to the next supplied variable
    }
    abi::emit_jump(emitter, &loop_label);
    emitter.label(&done_label);
}

/// Emits by-reference warnings for ordinary values in a named argument container.
pub(super) fn warn_assoc_nonreference_roots(
    emitter: &mut Emitter, ctx: &mut InvokerEmitContext, data: &mut DataSection, hash_reg: &str,
) {
    let (cursor, ordinal) = if emitter.target.arch == Arch::AArch64 {
        ("x23", "x24")
    } else {
        ("rbx", "r15")
    };
    let loop_label = ctx.next_label("mb_variables_hash_refs_loop");
    let done_label = ctx.next_label("mb_variables_hash_refs_done");
    let accepted_label = ctx.next_label("mb_variables_hash_ref_accepted");
    let check_label = ctx.next_label("mb_variables_hash_ref_check");
    let named_root_label = ctx.next_label("mb_variables_hash_named_root");
    let warn_label = ctx.next_label("mb_variables_hash_ref_warn");
    let numeric_warning_label = ctx.next_label("mb_variables_hash_ref_numeric_warn");
    abi::emit_reserve_temporary_stack(emitter, 16);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("str xzr, [sp]"),                  // start with a positional argument
        Arch::X86_64 => emitter.instruction("mov QWORD PTR [rsp], 0"),          // start with a positional argument
    }
    abi::emit_load_int_immediate(emitter, cursor, 0);
    abi::emit_load_int_immediate(emitter, ordinal, 0);
    emitter.label(&loop_label);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), hash_reg);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), cursor);
    abi::emit_call_label(emitter, "__rt_hash_iter_next");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmn x0, #1");                                  // stop after the last live entry
            emitter.instruction(&format!("b.eq {done_label}"));                 // no further callable arguments remain
            abi::emit_reg_move(emitter, cursor, "x0");
            emitter.instruction("cmn x2, #1");                                  // distinguish positional and named arguments
            emitter.instruction(&format!("b.eq {check_label}"));                // positional roots follow their source ordinal
            emitter.instruction("cmp x2, #3");                                  // only the named $var argument can be a root here
            emitter.instruction(&format!("b.ne {accepted_label}"));             // encoding names require no reference
            emitter.instruction("ldrb w9, [x1]");                               // compare the first byte of var
            emitter.instruction("cmp w9, #118");                                // ASCII v
            emitter.instruction(&format!("b.ne {accepted_label}"));             // another name is not the variable root
            emitter.instruction("ldrb w9, [x1, #1]");                           // compare the second byte of var
            emitter.instruction("cmp w9, #97");                                 // ASCII a
            emitter.instruction(&format!("b.ne {accepted_label}"));             // another name is not the variable root
            emitter.instruction("ldrb w9, [x1, #2]");                           // compare the final byte of var
            emitter.instruction("cmp w9, #114");                                // ASCII r
            emitter.instruction(&format!("b.ne {accepted_label}"));             // another name is not the variable root
            emitter.instruction("mov x9, #1");                                  // remember that PHP reports named $var as argument three
            abi::emit_store_to_sp(emitter, "x9", 0);
            emitter.instruction(&format!("b {check_label}"));                   // inspect the named root's value
            emitter.label(&check_label);
            emitter.instruction("ldr x9, [sp]");                                // keep named $var even when it arrived before encodings
            emitter.instruction(&format!("cbnz x9, {named_root_label}"));       // named roots skip the positional prefix check
            emitter.instruction("cmp x24, #2");                                 // the first two positional entries are encodings
            emitter.instruction(&format!("b.lt {accepted_label}"));             // only variable roots require references
            emitter.label(&named_root_label);
            emitter.instruction("cmp x5, #11");                                 // raw hash references already own caller storage
            emitter.instruction(&format!("b.eq {accepted_label}"));             // keep the original reference cell
            emitter.instruction("cmp x5, #7");                                  // a boxed value may hide a reference marker
            emitter.instruction(&format!("b.ne {warn_label}"));                 // ordinary raw values need a warning
            emitter.instruction("ldr x9, [x3]");                                // inspect the boxed cell without dereferencing it
            emitter.instruction("cmp x9, #11");                                 // boxed PHP reference marker
            emitter.instruction(&format!("b.eq {accepted_label}"));             // pass its original cell
            emitter.instruction("cmp x9, #7");                                  // persistent reference wrapper kind
            emitter.instruction(&format!("b.ne {warn_label}"));                 // other nested boxes are ordinary values
            emitter.instruction("ldr x9, [x3, #16]");                           // inspect the persistent reference flag
            emitter.instruction("cmp x9, #1");                                  // flag one denotes a live reference
            emitter.instruction(&format!("b.eq {accepted_label}"));             // retain its writable target
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, -1");                                 // stop after the last live entry
            emitter.instruction(&format!("je {done_label}"));                   // no further callable arguments remain
            abi::emit_reg_move(emitter, cursor, "rax");
            emitter.instruction("cmp rdx, -1");                                 // distinguish positional and named arguments
            emitter.instruction(&format!("je {check_label}"));                  // positional roots follow their source ordinal
            emitter.instruction("cmp rdx, 3");                                  // only the named $var argument can be a root here
            emitter.instruction(&format!("jne {accepted_label}"));              // encoding names require no reference
            emitter.instruction("cmp BYTE PTR [rdi], 118");                     // compare the first byte of var
            emitter.instruction(&format!("jne {accepted_label}"));              // another name is not the variable root
            emitter.instruction("cmp BYTE PTR [rdi + 1], 97");                  // compare the second byte of var
            emitter.instruction(&format!("jne {accepted_label}"));              // another name is not the variable root
            emitter.instruction("cmp BYTE PTR [rdi + 2], 114");                 // compare the final byte of var
            emitter.instruction(&format!("jne {accepted_label}"));              // another name is not the variable root
            emitter.instruction("mov QWORD PTR [rsp], 1");                      // remember that PHP reports named $var as argument three
            emitter.instruction(&format!("jmp {check_label}"));                 // inspect the named root's value
            emitter.label(&check_label);
            emitter.instruction("cmp QWORD PTR [rsp], 0");                      // keep named $var even when it arrived before encodings
            emitter.instruction(&format!("jne {named_root_label}"));            // named roots skip the positional prefix check
            emitter.instruction("cmp r15, 2");                                  // the first two positional entries are encodings
            emitter.instruction(&format!("jl {accepted_label}"));               // only variable roots require references
            emitter.label(&named_root_label);
            emitter.instruction("cmp r9, 11");                                  // raw hash references already own caller storage
            emitter.instruction(&format!("je {accepted_label}"));               // keep the original reference cell
            emitter.instruction("cmp r9, 7");                                   // a boxed value may hide a reference marker
            emitter.instruction(&format!("jne {warn_label}"));                  // ordinary raw values need a warning
            emitter.instruction("mov r10, QWORD PTR [rcx]");                    // inspect the boxed cell without dereferencing it
            emitter.instruction("cmp r10, 11");                                 // boxed PHP reference marker
            emitter.instruction(&format!("je {accepted_label}"));               // pass its original cell
            emitter.instruction("cmp r10, 7");                                  // persistent reference wrapper kind
            emitter.instruction(&format!("jne {warn_label}"));                  // other nested boxes are ordinary values
            emitter.instruction("cmp QWORD PTR [rcx + 16], 1");                 // flag one denotes a live reference
            emitter.instruction(&format!("je {accepted_label}"));               // retain its writable target
        }
    }
    emitter.label(&warn_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // choose the named $var position when present
            emitter.instruction(&format!("cbz x9, {numeric_warning_label}"));   // positional roots already carry their index
        }
        Arch::X86_64 => {
            emitter.instruction("cmp QWORD PTR [rsp], 0");                      // choose the named $var position when present
            emitter.instruction(&format!("je {numeric_warning_label}"));        // positional roots already carry their index
        }
    }
    abi::emit_store_to_sp(emitter, ordinal, 8);
    abi::emit_load_int_immediate(emitter, ordinal, 2);
    emit_missing_reference_warning(emitter, ctx, data, ordinal);
    abi::emit_load_temporary_stack_slot(emitter, ordinal, 8);
    abi::emit_jump(emitter, &accepted_label);
    emitter.label(&numeric_warning_label);
    emit_missing_reference_warning(emitter, ctx, data, ordinal);
    emitter.label(&accepted_label);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("str xzr, [sp]"),                  // reset the named-argument marker for the next entry
        Arch::X86_64 => emitter.instruction("mov QWORD PTR [rsp], 0"),          // reset the named-argument marker for the next entry
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("add x24, x24, #1"),               // track the next source argument position
        Arch::X86_64 => emitter.instruction("add r15, 1"),                      // track the next source argument position
    }
    abi::emit_jump(emitter, &loop_label);
    emitter.label(&done_label);
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Formats one by-value callable warning with the PHP argument position.
fn emit_missing_reference_warning(
    emitter: &mut Emitter, ctx: &mut InvokerEmitContext, data: &mut DataSection, cursor: &str,
) {
    let prefix = data.add_string(b"Warning: mb_convert_variables(): Argument #");
    let first_name = data.add_string(b" ($var)");
    let suffix = data.add_string(b" must be passed by reference, value given\n");
    let name_done = ctx.next_label("mb_variables_ref_name_done");
    let target = emitter.target;
    let scratch = if target.arch == Arch::AArch64 { "x9" } else { "r10" };
    abi::emit_reserve_temporary_stack(emitter, 16);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_concat_off", 0);
    abi::emit_store_to_sp(emitter, scratch, 0);
    emit_warning_fragment(emitter, &prefix, false);
    abi::emit_reg_move(emitter, abi::int_result_reg(emitter), cursor);
    match target.arch {
        Arch::AArch64 => emitter.instruction("add x0, x0, #1"),                 // PHP argument positions start at one
        Arch::X86_64 => emitter.instruction("add rax, 1"),                      // PHP argument positions start at one
    }
    abi::emit_call_label(emitter, "__rt_itoa");
    if target.arch == Arch::X86_64 {
        abi::emit_reg_move(emitter, "rdi", "rax");
        abi::emit_reg_move(emitter, "rsi", "rdx");
    }
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_load_temporary_stack_slot(emitter, scratch, 0);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_concat_off", 0);
    match target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {cursor}, #2"));                  // only the first variable has the named $var parameter
            emitter.instruction(&format!("b.ne {name_done}"));                  // later variadic variables have no parameter name
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {cursor}, 2"));                   // only the first variable has the named $var parameter
            emitter.instruction(&format!("jne {name_done}"));                   // later variadic variables have no parameter name
        }
    }
    emit_warning_fragment(emitter, &first_name, false);
    emitter.label(&name_done);
    emit_warning_fragment(emitter, &suffix, true);
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Appends a static warning fragment and finishes the diagnostic on its final piece.
fn emit_warning_fragment(emitter: &mut Emitter, fragment: &(String, usize), complete: bool) {
    let (pointer, length) = if emitter.target.arch == Arch::AArch64 {
        ("x1", "x2")
    } else {
        ("rdi", "rsi")
    };
    abi::emit_symbol_address(emitter, pointer, &fragment.0);
    abi::emit_load_int_immediate(emitter, length, fragment.1 as i64);
    abi::emit_call_label(emitter, if complete { "__rt_diag_warning" } else { "__rt_diag_warning_fragment" });
}

/// Borrows named values into stack cells and includes defaults only before the last supplied slot.
pub(super) fn emit_assoc(
    emitter: &mut Emitter, ctx: &mut InvokerEmitContext, data: &mut DataSection,
    source: LoadedArraySource, operation: RuntimeBuiltinId,
) {
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).expect("mbstring contract");
    let capacity = contract.params.len();
    let pointers = (capacity * 8 + 15) & !15;
    let count_slot = pointers + capacity * 24;
    let frame = (count_slot + 8 + 15) & !15;
    let target = emitter.target;
    let (hash, scratch) = match target.arch { Arch::AArch64 => ("x20", "x9"), Arch::X86_64 => ("r13", "r10") };
    emit_loaded_array_source_to_reg(source, hash, emitter);
    abi::emit_reserve_temporary_stack(emitter, frame);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    abi::emit_store_to_sp(emitter, scratch, count_slot);
    for (index, parameter) in contract.params.iter().enumerate() {
        let record = pointers + index * 24;
        let missing = ctx.next_label("mbstring_named_default");
        let ready = ctx.next_label("mbstring_named_value_ready");
        emit_hash_lookup_for_param_or_index(hash, Some(parameter.name), index, emitter, ctx, data);
        abi::emit_branch_if_int_result_zero(emitter, &missing);
        let (low, high, tag) = raw_hash_value_regs(emitter);
        abi::emit_store_to_sp(emitter, tag, record);
        abi::emit_store_to_sp(emitter, low, record + 8);
        abi::emit_store_to_sp(emitter, high, record + 16);
        abi::emit_load_int_immediate(emitter, scratch, index as i64 + 1);
        abi::emit_store_to_sp(emitter, scratch, count_slot);
        abi::emit_jump(emitter, &ready);
        emitter.label(&missing);
        stage_default(emitter, data, parameter.default, record);
        emitter.label(&ready);
        abi::emit_temporary_stack_address(emitter, scratch, record);
        abi::emit_store_to_sp(emitter, scratch, index * 8);
    }
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 0), operation.as_u32() as i64);
    abi::emit_temporary_stack_address(emitter, abi::int_arg_reg_name(target, 1), 0);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(target, 2), count_slot);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 3), 0);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 4), 0);
    abi::emit_call_label(emitter, "__rt_mbstring_native");
    abi::emit_release_temporary_stack(emitter, frame);
    abi::emit_call_label(emitter, "__rt_mbstring_box_result");
}

/// Writes an immutable scalar default using neutral metadata without allocating temporary owners.
fn stage_default(
    emitter: &mut Emitter, data: &mut DataSection,
    default: Option<elephc_builtin_contract::DefaultSpec>, record: usize,
) {
    use elephc_builtin_contract::DefaultSpec;
    let (ty, low) = match default {
        None | Some(DefaultSpec::Null) => (PhpType::Void, 0),
        Some(DefaultSpec::Int(value)) => (PhpType::Int, value),
        Some(DefaultSpec::Bool(value)) => (PhpType::Bool, i64::from(value)),
        Some(DefaultSpec::Str(_) | DefaultSpec::EmptyArray) => (PhpType::Str, 0),
        _ => unreachable!("mbstring contracts use scalar literal defaults"),
    };
    let scratch = match emitter.target.arch { Arch::AArch64 => "x9", Arch::X86_64 => "r10" };
    abi::emit_load_int_immediate(emitter, scratch, crate::codegen::runtime_value_tag(&ty) as i64);
    abi::emit_store_to_sp(emitter, scratch, record);
    // An omitted empty header array is observationally the same as an empty header string.
    // Borrowing that literal avoids creating a heap array solely for callable gap filling.
    let string_default = match default {
        Some(DefaultSpec::Str(value)) => Some(value),
        Some(DefaultSpec::EmptyArray) => Some(""),
        _ => None,
    };
    if let Some(value) = string_default {
        let (label, length) = data.add_string(value.as_bytes());
        abi::emit_symbol_address(emitter, scratch, &label);
        abi::emit_store_to_sp(emitter, scratch, record + 8);
        abi::emit_load_int_immediate(emitter, scratch, length as i64);
    } else {
        abi::emit_load_int_immediate(emitter, scratch, low);
        abi::emit_store_to_sp(emitter, scratch, record + 8);
        abi::emit_load_int_immediate(emitter, scratch, 0);
    }
    abi::emit_store_to_sp(emitter, scratch, record + 16);
}

/// Stages aligned borrowed pointers without retaining arguments or copying an owned string return.
pub(super) fn emit_indexed(
    emitter: &mut Emitter, ctx: &mut InvokerEmitContext,
    source: LoadedArraySource, operation: RuntimeBuiltinId,
) {
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id()).expect("mbstring contract");
    let capacity = contract.max_args.unwrap_or(contract.params.len());
    let bytes = (capacity * 8 + 15) & !15;
    let target = emitter.target;
    let array_reg = match target.arch { Arch::AArch64 => "x9", Arch::X86_64 => "r10" };
    emit_loaded_array_source_to_reg(source, array_reg, emitter);
    abi::emit_reserve_temporary_stack(emitter, bytes);
    abi::emit_load_from_address(emitter, abi::int_arg_reg_name(target, 2), array_reg, 0);
    let ready = ctx.next_label("mbstring_pointers_ready");
    for index in 0..capacity {
        match target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp x2, #{index}"));              // preserve the actual argument count before bounded pointer staging
                emitter.instruction(&format!("b.ls {ready}"));                  // leave omitted arguments absent for the shared coordinator
                emitter.instruction(&format!("ldr x10, [x9, #{}]", 24 + index * 8)); // read one borrowed Mixed cell from the native array
                emitter.instruction(&format!("str x10, [sp, #{}]", index * 8)); // provide an aligned pointer array for the Rust boundary
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp rdx, {index}"));              // retain actual arity independently of the bounded staging area
                emitter.instruction(&format!("jbe {ready}"));                   // skip absent arguments without filling defaults
                emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {}]", 24 + index * 8)); // borrow the original cell without an extra owner
                emitter.instruction(&format!("mov QWORD PTR [rsp + {}], r11", index * 8)); // align the pointer slice consumed by Rust
            }
        }
    }
    emitter.label(&ready);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 0), operation.as_u32() as i64);
    abi::emit_temporary_stack_address(emitter, abi::int_arg_reg_name(target, 1), 0);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 3), 0);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(target, 4), 0);
    abi::emit_call_label(emitter, "__rt_mbstring_native");
    abi::emit_release_temporary_stack(emitter, bytes);
    abi::emit_call_label(emitter, "__rt_mbstring_box_result");
}
