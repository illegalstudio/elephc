//! Purpose:
//! Lowers IPv4 and binary-address conversion string builtins.
//!
//! Called from:
//! - The string builtin lowering facade.
//!
//! Key details:
//! - Invalid address sentinels are boxed as PHP false on both supported architectures.

use super::*;

/// Lowers `long2ip(value)` through the IPv4 formatting runtime helper.
pub(crate) fn lower_long2ip(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "long2ip", 1)?;
    let value = expect_operand(inst, 0)?;
    load_as_int(ctx, value, "long2ip")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the IPv4 integer to the formatter helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_long2ip");
    store_if_result(ctx, inst)
}

/// Lowers `ip2long(string)` and boxes invalid-address results as PHP false.
pub(crate) fn lower_ip2long(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "ip2long")?;
    move_string_result_to_c_abi_pair(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_ip2long");
    box_ip2long_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers the raw `(int)ip2long()` path; invalid addresses become zero.
pub(crate) fn lower_ip2long_int(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    load_single_string_arg(ctx, inst, "ip2long")?;
    move_string_result_to_c_abi_pair(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_ip2long");
    let ok = ctx.next_label("ip2long_int_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // does the parser report an invalid address?
            ctx.emitter.instruction(&format!("b.ge {ok}"));                     // valid addresses keep the parsed integer
            ctx.emitter.instruction("mov x0, #0");                              // PHP's (int)false is zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // does the parser report an invalid address?
            ctx.emitter.instruction(&format!("jns {ok}"));                      // valid addresses keep the parsed integer
            ctx.emitter.instruction("xor eax, eax");                            // PHP's (int)false is zero
        }
    }
    ctx.emitter.label(&ok);
    store_if_result(ctx, inst)
}

/// Lowers `inet_ntop()` and `inet_pton()` and boxes invalid-address results as PHP false.
pub(crate) fn lower_inet(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    lower_inet_raw(ctx, inst, name, runtime_label, 0, true)?;
    store_if_result(ctx, inst)
}

/// Lowers the allocation-free `strlen(inet_pton(...))` helper.
///
/// The packed address is still parsed and validated, but the result is returned as
/// a bare 4/16-byte count (or zero for an invalid address) instead of a boxed
/// `string|false` cell.
pub(crate) fn lower_inet_pton_len(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_inet_raw(ctx, inst, "inet_pton", "__rt_inet_pton", 1, false)?;
    store_if_result(ctx, inst)
}

/// Emits the shared address materialization, mode selection and helper call.
fn lower_inet_raw(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
    mode: i64,
    box_result: bool,
) -> Result<()> {
    load_single_string_arg(ctx, inst, name)?;
    move_string_result_to_c_abi_pair(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x2, #{mode}"));               // 0 = boxed string, 1 = packed length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov r8d, {mode}"));               // 0 = boxed string, 1 = packed length
        }
    }
    abi::emit_call_label(ctx.emitter, runtime_label);
    if box_result {
        box_string_or_false_result(ctx, name);
    }
    Ok(())
}
/// Moves the standard string result pair into the C-style pointer/length argument pair.
pub(super) fn move_string_result_to_c_abi_pair(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the string pointer as the first C ABI argument
            ctx.emitter.instruction("mov x1, x2");                              // pass the string length as the second C ABI argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the string pointer as the first SysV argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the string length as the second SysV argument
        }
    }
}

/// Boxes an `ip2long()` integer result or invalid-address sentinel into Mixed form.
pub(super) fn box_ip2long_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("ip2long_false");
    let done_label = ctx.next_label("ip2long_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test whether ip2long() returned the invalid-address sentinel
            ctx.emitter.instruction(&format!("b.lt {}", false_label));          // box PHP false for invalid addresses
            ctx.emitter.instruction("mov x1, x0");                              // pass the parsed IPv4 integer as the Mixed payload
            ctx.emitter.instruction("mov x2, #0");                              // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after a valid parse
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // false payload = 0 for invalid addresses
            ctx.emitter.instruction("mov x2, #0");                              // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = boolean
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether ip2long() returned the invalid-address sentinel
            ctx.emitter.instruction(&format!("js {}", false_label));            // box PHP false for invalid addresses
            ctx.emitter.instruction("mov rdi, rax");                            // pass the parsed IPv4 integer as the Mixed payload
            ctx.emitter.instruction("xor esi, esi");                            // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // runtime tag 0 = integer
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after a valid parse
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // false payload = 0 for invalid addresses
            ctx.emitter.instruction("xor esi, esi");                            // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // runtime tag 3 = boolean
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a string result or null-pointer failure sentinel into Mixed form.
pub(super) fn box_string_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    box_string_or_false_with_ownership(ctx, label_prefix, false);
}

/// Transfers an owned string or boxes its null-pointer failure sentinel into Mixed form.
pub(super) fn box_owned_string_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    box_string_or_false_with_ownership(ctx, label_prefix, true);
}

/// Shares sentinel handling while retaining the source string ownership contract.
fn box_string_or_false_with_ownership(ctx: &mut FunctionContext<'_>, label_prefix: &str, owned: bool) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- select string or false boxing from the pointer sentinel --
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // a null string pointer means the conversion failed
            // -- transfer owned output or persist borrowed string storage --
            if owned {
                crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Str);
            } else {
                crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            }
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after a valid string result
            // -- materialize PHP false for the failure sentinel --
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // false payload = 0 for failed conversion
            ctx.emitter.instruction("mov x2, #0");                              // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = boolean
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            // -- select string or false boxing from the pointer sentinel --
            ctx.emitter.instruction("test rax, rax");                           // a null string pointer means the conversion failed
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box PHP false for failed conversions
            // -- transfer owned output or persist borrowed string storage --
            if owned {
                crate::codegen::emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Str);
            } else {
                crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            }
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after a valid string result
            // -- materialize PHP false for the failure sentinel --
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // false payload = 0 for failed conversion
            ctx.emitter.instruction("xor esi, esi");                            // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // runtime tag 3 = boolean
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}
