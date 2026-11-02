//! Purpose:
//! Shared runtime tag-dispatch `\TypeError` machinery for union-boxed array-family builtin
//! readers. A boxed `Mixed` local produced by an Elvis/ternary join
//! (`$u = $hosts ?: false`) is a CORRECTLY tagged cell (verified via `--emit-ir`: both join arms
//! box through `mixed_box`/`__rt_mixed_from_array_kind` before the merge), so the fix for the
//! family's SIGSEGV belongs at the READ side, not the join — this module is that read-side seam.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_count_dynamic()`, which calls
//!   `emit_mixed_wrong_tag_type_error_dispatch` directly (its accepted-tag set — 4/5/6 for
//!   array/hash/Countable-object — requires its own tag probe before the shared dispatch).
//! - Other gradual builtin and language-operation guards that need the same per-tag diagnostic.
//!
//! Key details:
//! - `emit_mixed_wrong_tag_type_error_dispatch` is the reusable per-tag (int/string/float/
//!   true/false/null/generic-other) dispatch + catchable-`\TypeError`-throw sequence. It mirrors
//!   `array_unshift`'s own dynamic-union wrong-tag dispatch
//!   (`crate::codegen::lower_inst::builtins::arrays::unshift::emit_array_unshift_union_wrong_tag_dispatch`)
//!   but is reusable across every reader in this family instead of being private to one builtin.
//! - Every throw reuses the standard heap-allocation/publish/unwind sequence established by
//!   `crate::codegen::lower_inst::objects::reflection::emit_reflection_class_argument_type_error_throw`
//!   (also mirrored by `unshift`'s own throw builder); the message text is supplied per call site
//!   via a closure so each builtin can match PHP's exact wording (parameter name, position, and
//!   declared type differ per builtin).

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::data_section::DataWord;
use crate::codegen::platform::Arch;
use crate::codegen::Result;

/// Emits the reusable wrong-tag `\TypeError` dispatch for a union-boxed array-family argument.
///
/// Entered via a jump when the argument's post-`__rt_mixed_unbox` runtime tag was not the single
/// accepted tag (4 = indexed array); the tag is still live in the unbox output register
/// (`x0`/`rax`) and the payload low word in `x1`/`rdi` (needed to tell PHP `true` from `false`).
/// Builds and throws a catchable `\TypeError` for whichever concrete PHP type the tag represents,
/// using `message_for(given_type_name)` to build the exact per-builtin wording. Never returns.
pub(in crate::codegen::lower_inst) fn emit_mixed_wrong_tag_type_error_dispatch(
    ctx: &mut FunctionContext<'_>,
    entry_label: &str,
    message_for: &impl Fn(&str) -> String,
) {
    // The common case is one call: the eleven per-type raises differ only in the "X given" word,
    // so a module that shares its throwables sends the tag and payload to ONE helper that spells
    // the word at run time. Measured on a probe of forty typed-parameter sites, the inlined
    // ladder is ~60 instructions and eleven interned throw records per site.
    if let Some((prefix, suffix)) = split_given_message(message_for) {
        if let Some(helper) = crate::codegen::shared_type_error_given::shared_label(ctx) {
            let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
            let (suffix_label, suffix_len) = if suffix.is_empty() {
                (prefix_label.clone(), 0)
            } else {
                ctx.data.add_string(suffix.as_bytes())
            };
            let record = ctx.data.add_words(vec![
                DataWord::Symbol(prefix_label),
                DataWord::U64(prefix_len as u64),
                DataWord::Symbol(suffix_label),
                DataWord::U64(suffix_len as u64),
            ]);
            ctx.emitter.label(entry_label);
            abi::emit_symbol_address(ctx.emitter, record_reg(ctx), &record);
            abi::emit_call_label(ctx.emitter, helper);
            return;
        }
    }
    let int_label = ctx.next_label("mixed_array_guard_te_int");
    let string_label = ctx.next_label("mixed_array_guard_te_string");
    let float_label = ctx.next_label("mixed_array_guard_te_float");
    let bool_label = ctx.next_label("mixed_array_guard_te_bool");
    let true_label = ctx.next_label("mixed_array_guard_te_true");
    let false_label = ctx.next_label("mixed_array_guard_te_false");
    let null_label = ctx.next_label("mixed_array_guard_te_null");
    let array_label = ctx.next_label("mixed_array_guard_te_array");
    let object_label = ctx.next_label("mixed_array_guard_te_object");
    let resource_label = ctx.next_label("mixed_array_guard_te_resource");
    let callable_label = ctx.next_label("mixed_array_guard_te_callable");
    let generic_label = ctx.next_label("mixed_array_guard_te_generic");

    ctx.emitter.label(entry_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // runtime tag 0 = int
            ctx.emitter.instruction(&format!("b.eq {}", int_label));
            ctx.emitter.instruction("cmp x0, #1");                              // runtime tag 1 = string
            ctx.emitter.instruction(&format!("b.eq {}", string_label));
            ctx.emitter.instruction("cmp x0, #2");                              // runtime tag 2 = float
            ctx.emitter.instruction(&format!("b.eq {}", float_label));
            ctx.emitter.instruction("cmp x0, #3");                              // runtime tag 3 = bool
            ctx.emitter.instruction(&format!("b.eq {}", bool_label));
            ctx.emitter.instruction("cmp x0, #8");                              // runtime tag 8 = null
            ctx.emitter.instruction(&format!("b.eq {}", null_label));
            ctx.emitter.instruction("cmp x0, #4");                              // runtime tag 4 = indexed array
            ctx.emitter.instruction(&format!("b.eq {}", array_label));
            ctx.emitter.instruction("cmp x0, #5");                              // runtime tag 5 = associative array
            ctx.emitter.instruction(&format!("b.eq {}", array_label));
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 = object
            ctx.emitter.instruction(&format!("b.eq {}", object_label));
            ctx.emitter.instruction("cmp x0, #9");                              // runtime tag 9 = resource
            ctx.emitter.instruction(&format!("b.eq {}", resource_label));
            ctx.emitter.instruction("cmp x0, #10");                             // runtime tag 10 = callable descriptor
            ctx.emitter.instruction(&format!("b.eq {}", callable_label));
            ctx.emitter.instruction(&format!("b {}", generic_label));           // defensive fallback for an unknown runtime tag
            ctx.emitter.label(&bool_label);
            ctx.emitter.instruction("cmp x1, #0");                              // is the unboxed boolean payload false?
            ctx.emitter.instruction(&format!("b.eq {}", false_label));
            ctx.emitter.instruction(&format!("b {}", true_label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // runtime tag 0 = int
            ctx.emitter.instruction(&format!("je {}", int_label));
            ctx.emitter.instruction("cmp rax, 1");                              // runtime tag 1 = string
            ctx.emitter.instruction(&format!("je {}", string_label));
            ctx.emitter.instruction("cmp rax, 2");                              // runtime tag 2 = float
            ctx.emitter.instruction(&format!("je {}", float_label));
            ctx.emitter.instruction("cmp rax, 3");                              // runtime tag 3 = bool
            ctx.emitter.instruction(&format!("je {}", bool_label));
            ctx.emitter.instruction("cmp rax, 8");                              // runtime tag 8 = null
            ctx.emitter.instruction(&format!("je {}", null_label));
            ctx.emitter.instruction("cmp rax, 4");                              // runtime tag 4 = indexed array
            ctx.emitter.instruction(&format!("je {}", array_label));
            ctx.emitter.instruction("cmp rax, 5");                              // runtime tag 5 = associative array
            ctx.emitter.instruction(&format!("je {}", array_label));
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 = object
            ctx.emitter.instruction(&format!("je {}", object_label));
            ctx.emitter.instruction("cmp rax, 9");                              // runtime tag 9 = resource
            ctx.emitter.instruction(&format!("je {}", resource_label));
            ctx.emitter.instruction("cmp rax, 10");                             // runtime tag 10 = callable descriptor
            ctx.emitter.instruction(&format!("je {}", callable_label));
            ctx.emitter.instruction(&format!("jmp {}", generic_label));         // defensive fallback for an unknown runtime tag
            ctx.emitter.label(&bool_label);
            ctx.emitter.instruction("test rdi, rdi");                           // is the unboxed boolean payload false?
            ctx.emitter.instruction(&format!("je {}", false_label));
            ctx.emitter.instruction(&format!("jmp {}", true_label));
        }
    }

    emit_type_error_case(ctx, &int_label, "int", message_for);
    emit_type_error_case(ctx, &string_label, "string", message_for);
    emit_type_error_case(ctx, &float_label, "float", message_for);
    emit_type_error_case(ctx, &true_label, "true", message_for);
    emit_type_error_case(ctx, &false_label, "false", message_for);
    emit_type_error_case(ctx, &null_label, "null", message_for);
    emit_type_error_case(ctx, &array_label, "array", message_for);
    emit_type_error_case(ctx, &object_label, "object", message_for);
    emit_type_error_case(ctx, &resource_label, "resource", message_for);
    emit_type_error_case(ctx, &callable_label, "callable", message_for);
    emit_type_error_case(ctx, &generic_label, "mixed", message_for);
}

/// Emits one concrete wrong-tag branch: labels it, builds `message_for(given_type)`, and throws.
fn emit_type_error_case(
    ctx: &mut FunctionContext<'_>,
    case_label: &str,
    given_type: &str,
    message_for: &impl Fn(&str) -> String,
) {
    ctx.emitter.label(case_label);
    let message = message_for(given_type);
    emit_throw_static_type_error(ctx, &message);
}

/// Constructs and throws a catchable `\TypeError` through the canonical exception emitter.
///
/// The shared emitter owns the current throwable layout, object-handle publication, and the
/// uncaught-versus-active-handler split, so gradual array guards cannot drift from other EIR
/// runtime type checks.
pub(in crate::codegen::lower_inst::builtins) fn emit_throw_static_type_error(
    ctx: &mut FunctionContext<'_>,
    message: &str,
) {
    super::super::super::exceptions::emit_type_error(ctx, message);
}

/// The register the shared helper reads its message record from.
///
/// The tag and the payload already sit in the unbox output registers, so the record rides in the
/// first register the unbox leaves free on each target.
fn record_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x2",
        Arch::X86_64 => "rsi",
    }
}

/// Splits a per-type message into the text before and after the "given" word.
///
/// `None` unless the message is exactly `prefix + given + suffix` for every spelling the ladder
/// raises, which is the one shape the shared helper can rebuild.
fn split_given_message(message_for: &impl Fn(&str) -> String) -> Option<(String, String)> {
    const MARK: &str = "\u{1}";
    let probe = message_for(MARK);
    let (prefix, suffix) = probe.split_once(MARK)?;
    if suffix.contains(MARK) {
        return None;
    }
    for given in [
        "int", "string", "float", "true", "false", "null", "array", "object", "resource",
        "callable", "mixed", "Foo\\Bar",
    ] {
        if message_for(given) != format!("{prefix}{given}{suffix}") {
            return None;
        }
    }
    Some((prefix.to_string(), suffix.to_string()))
}

/// Emits the body of the shared wrong-tag helper.
///
/// Entered with the unboxed tag in the integer result register, the payload in the unbox payload
/// register and the address of a `[prefix ptr, prefix len, suffix ptr, suffix len]` record in
/// `record_reg()`. Spells PHP's name for the value (`true` and `false` by value, the class for an
/// object), builds `prefix + name + suffix` and raises the `\TypeError`. Never returns.
pub(in crate::codegen) fn emit_shared_type_error_given_body(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let record = record_reg(ctx);
    let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let tag = abi::int_result_reg(ctx.emitter);
    let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
    let done = ctx.next_label("type_error_given_done");
    let bool_label = ctx.next_label("type_error_given_bool");
    let object_label = ctx.next_label("type_error_given_object");
    let fallback_label = ctx.next_label("type_error_given_generic");
    abi::emit_push_reg(ctx.emitter, record);

    // (tag, spelling) pairs; arrays share one spelling for the indexed and associative tags.
    let arms: [(i64, &str); 9] = [
        (0, "int"),
        (1, "string"),
        (2, "float"),
        (8, "null"),
        (4, "array"),
        (5, "array"),
        (9, "resource"),
        (10, "Closure"), // a callable descriptor is a Closure for php: `must be of type array, Closure given`
        (-1, "mixed"),
    ];
    let mut arm_labels = Vec::new();
    for (tag_value, _) in arms.iter().filter(|(tag_value, _)| *tag_value >= 0) {
        let label = ctx.next_label("type_error_given_arm");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, #{}", tag, tag_value)); // does the runtime tag select this spelling?
                ctx.emitter.instruction(&format!("b.eq {}", label));
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", tag, tag_value)); // does the runtime tag select this spelling?
                ctx.emitter.instruction(&format!("je {}", label));
            }
        }
        arm_labels.push(label);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #3", tag));               // runtime tag 3 = bool
            ctx.emitter.instruction(&format!("b.eq {}", bool_label));
            ctx.emitter.instruction(&format!("cmp {}, #6", tag));               // runtime tag 6 = object
            ctx.emitter.instruction(&format!("b.eq {}", object_label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 3", tag));                // runtime tag 3 = bool
            ctx.emitter.instruction(&format!("je {}", bool_label));
            ctx.emitter.instruction(&format!("cmp {}, 6", tag));                // runtime tag 6 = object
            ctx.emitter.instruction(&format!("je {}", object_label));
        }
    }
    abi::emit_jump(ctx.emitter, &fallback_label);                               // defensive fallback for an unknown runtime tag

    let mut labels = arm_labels.into_iter();
    for (tag_value, name) in arms.iter() {
        if *tag_value < 0 {
            continue;
        }
        let label = labels.next().expect("one label per tag arm");
        ctx.emitter.label(&label);
        emit_given_name(ctx, name, (text_ptr, text_len));
        abi::emit_jump(ctx.emitter, &done);
    }

    // PHP prints a boolean by value: "false given" / "true given", never "bool given".
    ctx.emitter.label(&bool_label);
    let false_label = ctx.next_label("type_error_given_false");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", payload));           // is the unboxed boolean payload false?
            ctx.emitter.instruction(&format!("b.eq {}", false_label));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", payload, payload)); // is the unboxed boolean payload false?
            ctx.emitter.instruction(&format!("je {}", false_label));
        }
    }
    emit_given_name(ctx, "true", (text_ptr, text_len));
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&false_label);
    emit_given_name(ctx, "false", (text_ptr, text_len));
    abi::emit_jump(ctx.emitter, &done);

    // An object is named by the class it actually holds.
    ctx.emitter.label(&object_label);
    abi::emit_reg_move(ctx.emitter, tag, payload);                              // the class-name lookup reads the object through the result register
    super::super::super::runtime_class_messages::emit_runtime_class_name_to_string_result(
        ctx, tag, "object",
    );
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&fallback_label);
    emit_given_name(ctx, "mixed", (text_ptr, text_len));

    ctx.emitter.label(&done);
    // text = prefix . name . suffix, with the record reloaded around each concat.
    let (right_ptr, right_len) =
        super::super::super::runtime_class_messages::concat_right_operand_regs(ctx.emitter);
    // The record is reloaded through a scratch register that is neither concat operand pair.
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, right_ptr, text_ptr);                       // the name becomes the concat right operand
    abi::emit_reg_move(ctx.emitter, right_len, text_len);
    abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, 0);
    abi::emit_load_from_address(ctx.emitter, text_ptr, scratch, 0);
    abi::emit_load_from_address(ctx.emitter, text_len, scratch, 8);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, 0);
    abi::emit_load_from_address(ctx.emitter, right_ptr, scratch, 16);
    abi::emit_load_from_address(ctx.emitter, right_len, scratch, 24);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    super::super::super::exceptions::emit_type_error_from_string_result(ctx);
    Ok(())
}

/// Loads the static spelling `name` into the string-result registers.
fn emit_given_name(ctx: &mut FunctionContext<'_>, name: &str, regs: (&str, &str)) {
    let (label, len) = ctx.data.add_string(name.as_bytes());
    abi::emit_symbol_address(ctx.emitter, regs.0, &label);
    abi::emit_load_int_immediate(ctx.emitter, regs.1, len as i64);
}
