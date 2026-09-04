//! Purpose:
//! Handles eval bridge statuses, thrown values, and fatal diagnostics.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Diagnostics and process exit remain target-aware and ABI-stable.

use super::*;

/// Where php locates an `eval()` parse error: the call in the host script, and the line inside
/// the fragment.
#[derive(Clone, Copy)]
pub(super) struct EvalParseSite {
    /// The line the `eval()` call sits on, which is what php puts in `FILE(LINE)`.
    pub call_line: u32,
    /// The line inside the fragment, which php names after `: eval()'d code on line`.
    pub fragment_line: u32,
}

/// Emits a fatal diagnostic when the eval bridge reports any non-zero status.
///
/// This form is for the bridge calls that are not a user `eval()` at all — scope writes,
/// dynamic invocations, symbol queries. Their "fragment" is compiler-generated, so there is no
/// call site to name and the parse-error line falls back to php's shape without a location.
pub(super) fn emit_eval_status_check(ctx: &mut FunctionContext<'_>) {
    emit_eval_status_check_at(ctx, None);
}

/// [`emit_eval_status_check`] with the source location php would print for a parse error.
pub(super) fn emit_eval_status_check_at(
    ctx: &mut FunctionContext<'_>,
    site: Option<EvalParseSite>,
) {
    let ok_label = ctx.next_label("eval_status_ok");
    let parse_error_label = ctx.next_label("eval_status_parse_error");
    let throwable_label = ctx.next_label("eval_status_throwable");
    let unsupported_label = ctx.next_label("eval_status_unsupported");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &ok_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_PARSE_ERROR, &parse_error_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_UNCAUGHT_THROWABLE, &throwable_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_UNSUPPORTED, &unsupported_label);
    emit_eval_fatal_message(ctx, EVAL_RUNTIME_FATAL_MESSAGE);
    ctx.emitter.label(&parse_error_label);
    let parse_error = eval_parse_error_message(ctx, site);
    emit_eval_diagnostic(ctx, &parse_error, EVAL_PARSE_ERROR_EXIT_STATUS);
    ctx.emitter.label(&throwable_label);
    emit_eval_throw_current(ctx);
    ctx.emitter.label(&unsupported_label);
    emit_eval_fatal_message(ctx, EVAL_UNSUPPORTED_MESSAGE);
    ctx.emitter.label(&ok_label);
}

/// Composes the line php prints for an `eval()` fragment that does not parse.
///
/// Measured on `php -n` 8.5.6, `eval("1 +")` inside `/tmp/ev.php` at line 4:
///
/// ```text
/// \nParse error: syntax error, unexpected end of file in /tmp/ev.php(4) : eval()'d code on line 1\n
/// ```
///
/// The leading newline, the `Parse error:` prefix, the host file with the CALL line in
/// parentheses, and the `: eval()'d code on line N` tail are all reproduced. Two measured
/// details are NOT:
///
/// - php names the exact syntactic complaint, and there are at least four shapes:
///   `syntax error, unexpected end of file`, `syntax error, unexpected end of file, expecting
///   "("`, `syntax error, unexpected token ";"`, `Unclosed '('` and `Unmatched '}'`. Which one
///   applies is a property of php's bison grammar; the bridge answers a status CODE and nothing
///   else, so elephc always writes the first — the one a truncated fragment earns, which is the
///   overwhelmingly common way an `eval()` fragment fails.
/// - php writes this to STDOUT under the CLI's default `display_errors`; elephc writes every
///   fatal to stderr and this one is not made an exception.
fn eval_parse_error_message(ctx: &FunctionContext<'_>, site: Option<EvalParseSite>) -> String {
    let (call_line, fragment_line) = match site {
        Some(site) => (Some(site.call_line), site.fragment_line),
        None => (None, 1),
    };
    let location = match (ctx.module.source_path.as_deref(), call_line) {
        (Some(path), Some(line)) => format!(" in {path}({line})"),
        _ => String::new(),
    };
    format!(
        "\n{EVAL_PARSE_ERROR_PREFIX}{location} : eval()'d code on line {fragment_line}\n"
    )
}

/// Branches to a label when the eval bridge returned a specific status code.
pub(super) fn emit_branch_if_eval_status(ctx: &mut FunctionContext<'_>, status: i64, label: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, #{}", result_reg, status)); // compare the eval bridge status against the handled code
            ctx.emitter.instruction(&format!("b.eq {}", label));                // branch to the matching eval status handler
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", result_reg, status)); // compare the eval bridge status against the handled code
            ctx.emitter.instruction(&format!("je {}", label));                  // branch to the matching eval status handler
        }
    }
}

/// Publishes an eval-thrown Throwable and enters the normal runtime unwinder.
pub(super) fn emit_eval_throw_current(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_ERROR_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let object_reg = eval_mixed_unbox_low_payload_reg(ctx);
    abi::emit_store_reg_to_symbol(ctx.emitter, object_reg, "_exc_value", 0);
    abi::emit_call_label(ctx.emitter, "__rt_throw_current");
}

/// Returns the low payload register produced by `__rt_mixed_unbox` for eval status handling.
pub(super) fn eval_mixed_unbox_low_payload_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    }
}

/// Emits an eval diagnostic message and exits the process with elephc's fatal status.
pub(super) fn emit_eval_fatal_message(ctx: &mut FunctionContext<'_>, message: &str) {
    emit_eval_diagnostic(ctx, message, EVAL_FATAL_EXIT_STATUS);
}

/// Emits an eval diagnostic message and exits the process with `status`.
fn emit_eval_diagnostic(ctx: &mut FunctionContext<'_>, message: &str, status: u32) {
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the eval runtime diagnostic to stderr
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the eval runtime diagnostic byte length
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, status);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the eval runtime diagnostic to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter
                .instruction(&format!("mov edx, {}", message_len)); // pass the eval runtime diagnostic byte length
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the eval runtime diagnostic before exiting
            abi::emit_exit(ctx.emitter, status);
        }
    }
}
