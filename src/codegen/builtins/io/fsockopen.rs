//! Purpose:
//! Emits PHP `fsockopen` calls.
//! Opens a connected TCP socket to a host/port pair and yields it as a PHP
//! stream resource, writing the optional by-reference error outputs.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Signature `fsockopen(hostname, port, &error_code, &error_message, timeout)`.
//!   The hostname string and port integer are handed to `__rt_fsockopen`, which
//!   builds the `tcp://host:port` address and connects through
//!   `__rt_stream_socket_client`.
//! - On success `&$error_code` is set to 0 and `&$error_message` to the empty
//!   string; on failure they are set to a generic connection error. The stores
//!   dispatch on the variable's storage class (global / by-ref param / local),
//!   matching the `stream_socket_recvfrom` write-back pattern.
//! - v1's documented limitation: the `$timeout` argument is evaluated for its
//!   side effects but the connection uses the OS default connect timeout.

use crate::codegen::builtins::io::stream_socket_server::box_socket_result;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fsockopen()");
    // PHP evaluates the value arguments left to right: hostname, port, then the
    // timeout. The error-code/message arguments are by-reference write targets,
    // so they are not evaluated as values here.
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(emitter, "x1", "x2"); // preserve the hostname string
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_push_reg(emitter, "x0"); // preserve the port
            if args.len() >= 5 {
                emit_expr(&args[4], emitter, ctx, data);
            }
            abi::emit_pop_reg(emitter, "x9"); // restore the port
            abi::emit_pop_reg_pair(emitter, "x1", "x2"); // restore the hostname
            emitter.instruction("mov x0, x1");                                  // arg 0 = hostname pointer
            emitter.instruction("mov x1, x2");                                  // arg 1 = hostname length
            emitter.instruction("mov x2, x9");                                  // arg 2 = port
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx"); // preserve the hostname string
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_push_reg(emitter, "rax"); // preserve the port
            if args.len() >= 5 {
                emit_expr(&args[4], emitter, ctx, data);
            }
            abi::emit_pop_reg(emitter, "r8"); // restore the port
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi"); // restore the hostname
            emitter.instruction("mov rdx, r8");                                 // arg 2 = port
        }
    }
    abi::emit_call_label(emitter, "__rt_fsockopen");
    emit_error_outputs(args, emitter, ctx, data);
    box_socket_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Writes the by-reference `&$error_code` / `&$error_message` outputs from the
/// connection result held in the integer result register. On success the code
/// is 0 and the message empty; on failure a generic connection error is
/// reported. The result register is preserved across the stores.
fn emit_error_outputs(args: &[Expr], emitter: &mut Emitter, ctx: &mut Context, data: &mut DataSection) {
    let errno_var = variable_name(args.get(2));
    let errstr_var = variable_name(args.get(3));
    if errno_var.is_none() && errstr_var.is_none() {
        return;
    }
    let (empty_sym, _) = data.add_string(b"");
    let (msg_sym, msg_len) = data.add_string(b"Connection refused");
    // The connect failure is not classified; report the platform's
    // ECONNREFUSED generically (the common cause).
    let econnrefused = emitter.platform.econnrefused();
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(emitter, "x0"); // preserve the connection result
            emitter.instruction("cmp x0, #0");                                  // did the connection succeed (fd >= 0)?
            emitter.instruction("mov x9, #0");                                  // success error code = 0
            emitter.instruction(&format!("mov x10, #{}", econnrefused));        // failure error code = ECONNREFUSED
            emitter.instruction("csel x9, x9, x10, ge");                        // x9 = error code for the outcome
            abi::emit_symbol_address(emitter, "x10", &msg_sym);
            abi::emit_symbol_address(emitter, "x11", &empty_sym);
            emitter.instruction("csel x10, x11, x10, ge");                      // x10 = error-message pointer
            emitter.instruction("mov x11, #0");                                 // success error-message length = 0
            emitter.instruction(&format!("mov x12, #{}", msg_len));             // failure error-message length
            emitter.instruction("csel x11, x11, x12, ge");                      // x11 = error-message length
            if let Some(name) = errno_var {
                store_int(name, "x9", emitter, ctx);
            }
            if let Some(name) = errstr_var {
                store_str(name, "x10", "x11", emitter, ctx);
            }
            abi::emit_pop_reg(emitter, "x0"); // restore the connection result
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax"); // preserve the connection result
            emitter.instruction("cmp rax, 0");                                  // did the connection succeed (fd >= 0)?
            emitter.instruction(&format!("mov r9, {}", econnrefused));          // failure error code = ECONNREFUSED
            emitter.instruction("mov r10, 0");                                  // success error code = 0
            emitter.instruction("cmovge r9, r10");                              // r9 = error code for the outcome
            emitter.instruction(&format!("lea r10, [rip + {}]", msg_sym));      // failure error-message pointer
            emitter.instruction(&format!("lea r11, [rip + {}]", empty_sym));    // success error-message pointer
            emitter.instruction("cmovge r10, r11");                             // r10 = error-message pointer
            emitter.instruction(&format!("mov r11, {}", msg_len));              // failure error-message length
            emitter.instruction("mov rcx, 0");                                  // success error-message length = 0
            emitter.instruction("cmovge r11, rcx");                             // r11 = error-message length
            if let Some(name) = errno_var {
                store_int(name, "r9", emitter, ctx);
            }
            if let Some(name) = errstr_var {
                store_str(name, "r10", "r11", emitter, ctx);
            }
            abi::emit_pop_reg(emitter, "rax"); // restore the connection result
        }
    }
}

/// Returns the variable name when `arg` is a plain `$variable` expression.
fn variable_name(arg: Option<&Expr>) -> Option<&str> {
    match arg.map(|a| &a.kind) {
        Some(ExprKind::Variable(name)) => Some(name.as_str()),
        _ => None,
    }
}

/// Stores a scalar register into a variable's 8-byte slot, dispatching on the
/// variable's storage class.
fn store_int(name: &str, value_reg: &str, emitter: &mut Emitter, ctx: &Context) {
    let is_global =
        ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name));
    if is_global {
        let label = format!("_gvar_{}", name);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.adrp("x13", &label);                                    // load page of the error-code variable
                emitter.add_lo12("x13", "x13", &label);                         // resolve the error-code variable
                emitter.instruction(&format!("str {}, [x13]", value_reg));      // store the error code
            }
            Arch::X86_64 => {
                abi::emit_store_reg_to_symbol(emitter, value_reg, &label, 0);    // store the error code
            }
        }
        return;
    }
    if ctx.ref_params.contains(name) {
        let offset = ctx
            .variables
            .get(name)
            .expect("codegen bug: missing ref-param slot for fsockopen error output")
            .stack_offset;
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x13", offset);                    // load the referenced error-code storage
                emitter.instruction(&format!("str {}, [x13]", value_reg));      // store the error code
            }
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "r13", offset);                    // load the referenced error-code storage
                abi::emit_store_to_address(emitter, value_reg, "r13", 0);       // store the error code
            }
        }
        return;
    }
    if let Some(offset) = ctx.variables.get(name).map(|var| var.stack_offset) {
        abi::store_at_offset(emitter, value_reg, offset);                       // store the error code into the local slot
    }
}

/// Stores a string pointer/length pair into a variable's 16-byte string slot,
/// dispatching on the variable's storage class.
fn store_str(name: &str, ptr_reg: &str, len_reg: &str, emitter: &mut Emitter, ctx: &Context) {
    let is_global =
        ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name));
    if is_global {
        let label = format!("_gvar_{}", name);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.adrp("x13", &label);                                    // load page of the error-message variable
                emitter.add_lo12("x13", "x13", &label);                         // resolve the error-message variable
                emitter.instruction(&format!("str {}, [x13]", ptr_reg));        // store the error-message pointer
                emitter.instruction(&format!("str {}, [x13, #8]", len_reg));    // store the error-message length
            }
            Arch::X86_64 => {
                abi::emit_store_reg_to_symbol(emitter, ptr_reg, &label, 0);      // store the error-message pointer
                abi::emit_store_reg_to_symbol(emitter, len_reg, &label, 8);      // store the error-message length
            }
        }
        return;
    }
    if ctx.ref_params.contains(name) {
        let offset = ctx
            .variables
            .get(name)
            .expect("codegen bug: missing ref-param slot for fsockopen error output")
            .stack_offset;
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x13", offset);                    // load the referenced error-message storage
                emitter.instruction(&format!("str {}, [x13]", ptr_reg));        // store the error-message pointer
                emitter.instruction(&format!("str {}, [x13, #8]", len_reg));    // store the error-message length
            }
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "r13", offset);                    // load the referenced error-message storage
                abi::emit_store_to_address(emitter, ptr_reg, "r13", 0);         // store the error-message pointer
                abi::emit_store_to_address(emitter, len_reg, "r13", 8);         // store the error-message length
            }
        }
        return;
    }
    if let Some(offset) = ctx.variables.get(name).map(|var| var.stack_offset) {
        // A local string slot keeps the pointer at `offset` and the length at
        // `offset - 8`, matching `abi::emit_store`/`emit_load` for `PhpType::Str`.
        abi::store_at_offset(emitter, ptr_reg, offset);                         // store the error-message pointer
        abi::store_at_offset(emitter, len_reg, offset - 8);                     // store the error-message length
    }
}
