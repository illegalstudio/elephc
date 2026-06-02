//! Purpose:
//! Unboxes PHP stream resources for file-handle based builtin emitters.
//! Emits consistent fatal/type-error paths when a stream argument is not valid.
//!
//! Called from:
//! - `crate::codegen::builtins::io::*::emit() for stream builtins`.
//!
//! Key details:
//! - Resource handles are runtime-owned file descriptors; validation must happen before syscall/helper use.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits argument expression and validates it as a stream resource.
///
/// Emits `arg` via `emit_expr`. If the resulting type is `Mixed` or `Union`,
/// emits `emit_unbox_stream_or_fatal` to unbox the resource and produce a fatal
/// TypeError at runtime if the value is not a valid stream. Returns the PHP type
/// of the argument expression.
///
/// # Arguments
/// * `function_name` - PHP builtin name used in error messages
/// * `arg` - The argument expression to emit and validate
/// * `emitter` - Target-specific assembly emitter
/// * `ctx` - Codegen context (label generation)
/// * `data` - Data section for string/constant emission
///
/// # Returns
/// The `PhpType` of the emitted argument expression.
pub(crate) fn emit_stream_fd_arg(
    function_name: &str,
    arg: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let ty = emit_expr(arg, emitter, ctx, data);
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_unbox_stream_or_fatal(function_name, emitter, ctx, data);
    }
    ty
}

/// Unboxes a Mixed/Union stream value or emits a fatal TypeError.
///
/// Calls `__rt_mixed_unbox` to extract the runtime value, then checks the boxed
/// payload tag (tag 9 = stream resource). On success, copies the native file
/// descriptor from `x1`/`rdi` to the integer result register. On failure,
/// branches to `emit_stream_type_error` for the appropriate PHP TypeError.
///
/// # Arguments
/// * `function_name` - PHP builtin name used in error messages
/// * `emitter` - Target-specific assembly emitter
/// * `ctx` - Codegen context (label generation)
/// * `data` - Data section for string/constant emission
fn emit_unbox_stream_or_fatal(
    function_name: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let ok_label = ctx.next_label("stream_resource_ok");

    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // unwrap a resource|false handle returned by fopen()
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #9");                                  // is the boxed handle a stream resource payload?
            emitter.instruction(&format!("b.eq {}", ok_label));                 // continue only for resource values
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 9");                                  // is the boxed handle a stream resource payload?
            emitter.instruction(&format!("je {}", ok_label));                   // continue only for resource values
        }
    }
    emit_stream_type_error(function_name, emitter, ctx, data);
    emitter.label(&ok_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, x1");                                  // expose the unboxed native stream descriptor as the ordinary integer result
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, rdi");                                // expose the unboxed native stream descriptor as the ordinary integer result
        }
    }
}

/// Emits a fatal TypeError for a stream argument with an unexpected PHP type.
///
/// Dispatches to type-specific error case labels based on the unboxed runtime
/// tag from `__rt_mixed_unbox`. Each case calls `emit_stream_type_error_case`
/// to emit the error message and terminate.
///
/// # Arguments
/// * `function_name` - PHP builtin name used in error messages
/// * `emitter` - Target-specific assembly emitter
/// * `ctx` - Codegen context (label generation)
/// * `data` - Data section for string/constant emission
fn emit_stream_type_error(
    function_name: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let int_label = ctx.next_label("stream_type_error_int");
    let string_label = ctx.next_label("stream_type_error_string");
    let float_label = ctx.next_label("stream_type_error_float");
    let bool_label = ctx.next_label("stream_type_error_bool");
    let false_label = ctx.next_label("stream_type_error_false");
    let true_label = ctx.next_label("stream_type_error_true");
    let array_label = ctx.next_label("stream_type_error_array");
    let object_label = ctx.next_label("stream_type_error_object");
    let null_label = ctx.next_label("stream_type_error_null");
    let unknown_label = ctx.next_label("stream_type_error_unknown");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // did the bad stream value unwrap to an integer?
            emitter.instruction(&format!("b.eq {}", int_label));                // report PHP's int-given stream TypeError
            emitter.instruction("cmp x0, #1");                                  // did the bad stream value unwrap to a string?
            emitter.instruction(&format!("b.eq {}", string_label));             // report PHP's string-given stream TypeError
            emitter.instruction("cmp x0, #2");                                  // did the bad stream value unwrap to a float?
            emitter.instruction(&format!("b.eq {}", float_label));              // report PHP's float-given stream TypeError
            emitter.instruction("cmp x0, #3");                                  // did the bad stream value unwrap to a boolean?
            emitter.instruction(&format!("b.eq {}", bool_label));               // split boolean payloads into true/false diagnostics
            emitter.instruction("cmp x0, #4");                                  // did the bad stream value unwrap to an indexed array?
            emitter.instruction(&format!("b.eq {}", array_label));              // report PHP's array-given stream TypeError
            emitter.instruction("cmp x0, #5");                                  // did the bad stream value unwrap to an associative array?
            emitter.instruction(&format!("b.eq {}", array_label));              // associative arrays share PHP's array-given wording
            emitter.instruction("cmp x0, #6");                                  // did the bad stream value unwrap to an object?
            emitter.instruction(&format!("b.eq {}", object_label));             // report PHP's object-given stream TypeError
            emitter.instruction("cmp x0, #8");                                  // did the bad stream value unwrap to null?
            emitter.instruction(&format!("b.eq {}", null_label));               // report PHP's null-given stream TypeError
            emitter.instruction(&format!("b {}", unknown_label));               // fall back for unsupported boxed payload tags
            emitter.label(&bool_label);
            emitter.instruction("cmp x1, #0");                                  // is the unboxed boolean payload false?
            emitter.instruction(&format!("b.eq {}", false_label));              // report PHP's false-given stream TypeError
            emitter.instruction(&format!("b {}", true_label));                  // report PHP's true-given stream TypeError
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 0");                                  // did the bad stream value unwrap to an integer?
            emitter.instruction(&format!("je {}", int_label));                  // report PHP's int-given stream TypeError
            emitter.instruction("cmp rax, 1");                                  // did the bad stream value unwrap to a string?
            emitter.instruction(&format!("je {}", string_label));               // report PHP's string-given stream TypeError
            emitter.instruction("cmp rax, 2");                                  // did the bad stream value unwrap to a float?
            emitter.instruction(&format!("je {}", float_label));                // report PHP's float-given stream TypeError
            emitter.instruction("cmp rax, 3");                                  // did the bad stream value unwrap to a boolean?
            emitter.instruction(&format!("je {}", bool_label));                 // split boolean payloads into true/false diagnostics
            emitter.instruction("cmp rax, 4");                                  // did the bad stream value unwrap to an indexed array?
            emitter.instruction(&format!("je {}", array_label));                // report PHP's array-given stream TypeError
            emitter.instruction("cmp rax, 5");                                  // did the bad stream value unwrap to an associative array?
            emitter.instruction(&format!("je {}", array_label));                // associative arrays share PHP's array-given wording
            emitter.instruction("cmp rax, 6");                                  // did the bad stream value unwrap to an object?
            emitter.instruction(&format!("je {}", object_label));               // report PHP's object-given stream TypeError
            emitter.instruction("cmp rax, 8");                                  // did the bad stream value unwrap to null?
            emitter.instruction(&format!("je {}", null_label));                 // report PHP's null-given stream TypeError
            emitter.instruction(&format!("jmp {}", unknown_label));             // fall back for unsupported boxed payload tags
            emitter.label(&bool_label);
            emitter.instruction("test rdi, rdi");                               // is the unboxed boolean payload false?
            emitter.instruction(&format!("je {}", false_label));                // report PHP's false-given stream TypeError
            emitter.instruction(&format!("jmp {}", true_label));                // report PHP's true-given stream TypeError
        }
    }

    emit_stream_type_error_case(function_name, "int", &int_label, emitter, data);
    emit_stream_type_error_case(function_name, "string", &string_label, emitter, data);
    emit_stream_type_error_case(function_name, "float", &float_label, emitter, data);
    emit_stream_type_error_case(function_name, "false", &false_label, emitter, data);
    emit_stream_type_error_case(function_name, "true", &true_label, emitter, data);
    emit_stream_type_error_case(function_name, "array", &array_label, emitter, data);
    emit_stream_type_error_case(function_name, "object", &object_label, emitter, data);
    emit_stream_type_error_case(function_name, "null", &null_label, emitter, data);
    emit_stream_type_error_case(function_name, "unknown", &unknown_label, emitter, data);
}

/// Emits a single stream TypeError case for a given PHP type.
///
/// Formats the PHP TypeError message using `function_name` and `given_type`,
/// adds it to the data section, and emits a jump to
/// `emit_write_type_error_and_exit`.
///
/// # Arguments
/// * `function_name` - PHP builtin name used in the error message
/// * `given_type` - The PHP type that was incorrectly provided
/// * `case_label` - Label to branch here for this type case
/// * `emitter` - Target-specific assembly emitter
/// * `data` - Data section for string/constant emission
fn emit_stream_type_error_case(
    function_name: &str,
    given_type: &str,
    case_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let message = format!(
        "Fatal error: Uncaught TypeError: {}(): Argument #1 ($stream) must be of type resource, {} given\n",
        function_name, given_type
    );
    let (label, len) = data.add_string(message.as_bytes());
    emitter.label(case_label);
    emit_write_type_error_and_exit(&label, len, emitter);
}

/// Emits the stream TypeError diagnostic to stderr and exits with status 1.
///
/// Writes the formatted error message to stderr using the Linux `write` syscall,
/// then calls `exit` with status 1. Target-specific: ARM64 uses `syscall`
/// instruction; x86_64 uses `syscall` instruction.
///
/// # Arguments
/// * `label` - Data section label for the error message string
/// * `len` - Length of the error message string in bytes
/// * `emitter` - Target-specific assembly emitter
fn emit_write_type_error_and_exit(label: &str, len: usize, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // fd = stderr for the stream TypeError diagnostic
            emitter.adrp("x1", label);                                          // load the page that contains the stream TypeError diagnostic
            emitter.add_lo12("x1", "x1", label);                                // resolve the stream TypeError diagnostic address within that page
            emitter.instruction(&format!("mov x2, #{}", len));                  // pass the stream TypeError diagnostic length to write()
            emitter.syscall(4);
            emitter.instruction("mov x0, #1");                                  // exit status 1 indicates abnormal termination
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", label);                    // point the Linux write buffer at the stream TypeError diagnostic
            emitter.instruction(&format!("mov edx, {}", len));                  // pass the stream TypeError diagnostic length to write()
            emitter.instruction("mov edi, 2");                                  // fd = stderr for the stream TypeError diagnostic
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the stream TypeError diagnostic
            emitter.instruction("mov edi, 1");                                  // exit status 1 indicates abnormal termination
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate after reporting the stream TypeError diagnostic
        }
    }
}
