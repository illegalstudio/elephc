//! Purpose:
//! Emits runtime warning helpers for undefined integer and string array keys.
//! Formats missing key values while preserving concat scratch state where needed.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - The helper is warning-only: callers still materialize their own null fallback.
//! - `__rt_itoa` uses `_concat_buf`, so `_concat_off` is restored before returning.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

const UNDEFINED_ARRAY_KEY_PREFIX_LEN: usize = "Warning: Undefined array key ".len();
const UNDEFINED_ARRAY_KEY_QUOTE_LEN: usize = "\"".len();
const UNDEFINED_ARRAY_KEY_SUFFIX_LEN: usize = "\n".len();
const ARRAY_OFFSET_ON_NULL_LEN: usize = "Warning: Trying to access array offset on null\n".len();

/// The boxed-Mixed spellings of the same warning, by runtime tag.
///
/// php names the TYPE an offset was read through — and a bool by its VALUE, so `true` and
/// `false` are separate messages rather than one `bool`. Measured on `php -n` 8.5.6 through a
/// `mixed` parameter, one payload per line. A boxed STRING and a boxed ARRAY are not here: both
/// are legal reads that answer a value.
const OFFSET_ON_FALSE: &str = "Warning: Trying to access array offset on false\n";
const OFFSET_ON_TRUE: &str = "Warning: Trying to access array offset on true\n";
const OFFSET_ON_INT: &str = "Warning: Trying to access array offset on int\n";
const OFFSET_ON_FLOAT: &str = "Warning: Trying to access array offset on float\n";
const OFFSET_ON_RESOURCE: &str = "Warning: Trying to access array offset on resource\n";

/// Emits `__rt_warn_undefined_array_key_int` for the active target.
pub fn emit_undefined_array_key_warning(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_undefined_array_key_warning_x86_64(emitter);
        emit_undefined_array_key_string_warning_x86_64(emitter);
        emit_array_offset_on_null_warning_x86_64(emitter);
        emit_array_offset_on_tag_warning(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: undefined_array_key_warning ---");
    emitter.label_global("__rt_warn_undefined_array_key_int");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // reserve saved key, concat cursor, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable runtime warning frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the missing integer key across warning fragments
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // snapshot concat scratch state before formatting the key
    emitter.instruction("str x10, [sp, #8]");                                   // preserve the concat cursor across itoa

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "x1", "_diag_undefined_array_key_prefix");
    emitter.instruction(&format!("mov x2, #{}", UNDEFINED_ARRAY_KEY_PREFIX_LEN)); // pass the undefined-key warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning prefix

    // -- emit formatted key --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the missing integer key for decimal formatting
    abi::emit_call_label(emitter, "__rt_itoa");                                 // format the missing key into concat scratch
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the formatted missing-key value
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the pre-warning concat cursor
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x10, [x9]");                                       // restore concat scratch state for surrounding expressions

    // -- emit suffix --
    abi::emit_symbol_address(emitter, "x1", "_diag_undefined_array_key_suffix");
    emitter.instruction(&format!("mov x2, #{}", UNDEFINED_ARRAY_KEY_SUFFIX_LEN)); // pass the undefined-key warning suffix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning suffix

    // -- restore stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the runtime warning frame
    emitter.instruction("ret");                                                 // return to the array-miss caller

    emit_undefined_array_key_string_warning_aarch64(emitter);
    emit_array_offset_on_null_warning_aarch64(emitter);
    emit_array_offset_on_tag_warning(emitter);
}

/// Emits the fixed PHP warning used when an array-offset receiver is null on AArch64.
fn emit_array_offset_on_null_warning_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_offset_on_null_warning ---");
    emitter.label_global("__rt_warn_array_offset_on_null");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve frame linkage across the diagnostic call
    emitter.instruction("mov x29, sp");                                         // establish a stable warning helper frame
    abi::emit_symbol_address(emitter, "x1", "_diag_array_offset_on_null");
    emitter.instruction(&format!("mov x2, #{}", ARRAY_OFFSET_ON_NULL_LEN));     // pass the complete array-offset-on-null warning length
    abi::emit_call_label(emitter, "__rt_diag_warning");                       // emit or suppress the PHP warning
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame linkage
    emitter.instruction("ret");                                                 // return to the null-receiver fallback
}

/// Emits `__rt_warn_array_offset_on_tag(tag, value_lo)`, php's warning for a NON-container base.
///
/// Called from the boxed-Mixed offset read, whose receiver type is only known at run time: a
/// `stat()` that answered `false` reaches an ordinary `$s[0]` and php warns there. Tags this
/// helper does not name — string, array, hash, object — are legal reads and never reach it; a
/// tag it does not recognise returns without a word rather than inventing one.
pub fn emit_array_offset_on_tag_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: array_offset_on_tag_warning ---");
            emitter.label_global("__rt_warn_array_offset_on_tag");
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // preserve frame linkage across the diagnostic call
            emitter.instruction("mov x29, sp");                                 // establish a stable warning helper frame
            emitter.instruction("cmp x0, #0");                                  // tag 0 = int
            emitter.instruction("b.eq __rt_waot_int");
            emitter.instruction("cmp x0, #2");                                  // tag 2 = float
            emitter.instruction("b.eq __rt_waot_float");
            emitter.instruction("cmp x0, #3");                                  // tag 3 = bool
            emitter.instruction("b.eq __rt_waot_bool");
            emitter.instruction("cmp x0, #9");                                  // tag 9 = resource
            emitter.instruction("b.eq __rt_waot_resource");
            emitter.instruction("b __rt_waot_done");                            // any other payload is not one php names here
            emitter.label("__rt_waot_int");
            abi::emit_symbol_address(emitter, "x1", "_diag_array_offset_on_int");
            emitter.instruction(&format!("mov x2, #{}", OFFSET_ON_INT.len()));
            emitter.instruction("b __rt_waot_warn");
            emitter.label("__rt_waot_float");
            abi::emit_symbol_address(emitter, "x1", "_diag_array_offset_on_float");
            emitter.instruction(&format!("mov x2, #{}", OFFSET_ON_FLOAT.len()));
            emitter.instruction("b __rt_waot_warn");
            emitter.label("__rt_waot_resource");
            abi::emit_symbol_address(emitter, "x1", "_diag_array_offset_on_resource");
            emitter.instruction(&format!("mov x2, #{}", OFFSET_ON_RESOURCE.len()));
            emitter.instruction("b __rt_waot_warn");
            emitter.label("__rt_waot_bool");
            emitter.instruction("cbz x1, __rt_waot_false");                     // php names the VALUE, not the type
            abi::emit_symbol_address(emitter, "x1", "_diag_array_offset_on_true");
            emitter.instruction(&format!("mov x2, #{}", OFFSET_ON_TRUE.len()));
            emitter.instruction("b __rt_waot_warn");
            emitter.label("__rt_waot_false");
            abi::emit_symbol_address(emitter, "x1", "_diag_array_offset_on_false");
            emitter.instruction(&format!("mov x2, #{}", OFFSET_ON_FALSE.len()));
            emitter.label("__rt_waot_warn");
            abi::emit_call_label(emitter, "__rt_diag_warning");                 // emit or suppress the PHP warning
            emitter.label("__rt_waot_done");
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore frame linkage
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: array_offset_on_tag_warning ---");
            emitter.label_global("__rt_warn_array_offset_on_tag");
            emitter.instruction("push rbp");                                    // preserve the caller frame and align the diagnostic call
            emitter.instruction("mov rbp, rsp");                                // establish a stable warning helper frame
            emitter.instruction("mov r10, rsi");                                // the payload decides the bool word
            emitter.instruction("cmp rdi, 0");                                  // tag 0 = int
            emitter.instruction("je __rt_waot_int_x86");
            emitter.instruction("cmp rdi, 2");                                  // tag 2 = float
            emitter.instruction("je __rt_waot_float_x86");
            emitter.instruction("cmp rdi, 3");                                  // tag 3 = bool
            emitter.instruction("je __rt_waot_bool_x86");
            emitter.instruction("cmp rdi, 9");                                  // tag 9 = resource
            emitter.instruction("je __rt_waot_resource_x86");
            emitter.instruction("jmp __rt_waot_done_x86");                      // any other payload is not one php names here
            emitter.label("__rt_waot_int_x86");
            abi::emit_symbol_address(emitter, "rdi", "_diag_array_offset_on_int");
            emitter.instruction(&format!("mov esi, {}", OFFSET_ON_INT.len()));
            emitter.instruction("jmp __rt_waot_warn_x86");
            emitter.label("__rt_waot_float_x86");
            abi::emit_symbol_address(emitter, "rdi", "_diag_array_offset_on_float");
            emitter.instruction(&format!("mov esi, {}", OFFSET_ON_FLOAT.len()));
            emitter.instruction("jmp __rt_waot_warn_x86");
            emitter.label("__rt_waot_resource_x86");
            abi::emit_symbol_address(emitter, "rdi", "_diag_array_offset_on_resource");
            emitter.instruction(&format!("mov esi, {}", OFFSET_ON_RESOURCE.len()));
            emitter.instruction("jmp __rt_waot_warn_x86");
            emitter.label("__rt_waot_bool_x86");
            emitter.instruction("test r10, r10");                               // php names the VALUE, not the type
            emitter.instruction("jz __rt_waot_false_x86");
            abi::emit_symbol_address(emitter, "rdi", "_diag_array_offset_on_true");
            emitter.instruction(&format!("mov esi, {}", OFFSET_ON_TRUE.len()));
            emitter.instruction("jmp __rt_waot_warn_x86");
            emitter.label("__rt_waot_false_x86");
            abi::emit_symbol_address(emitter, "rdi", "_diag_array_offset_on_false");
            emitter.instruction(&format!("mov esi, {}", OFFSET_ON_FALSE.len()));
            emitter.label("__rt_waot_warn_x86");
            abi::emit_call_label(emitter, "__rt_diag_warning");                 // emit or suppress the PHP warning
            emitter.label("__rt_waot_done_x86");
            emitter.instruction("mov rsp, rbp");                                // release the warning helper frame
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");
        }
    }
}

/// Emits the fixed PHP warning used when an array-offset receiver is null on x86_64.
fn emit_array_offset_on_null_warning_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_offset_on_null_warning ---");
    emitter.label_global("__rt_warn_array_offset_on_null");
    emitter.instruction("push rbp");                                            // preserve the caller frame and align the diagnostic call
    emitter.instruction("mov rbp, rsp");                                        // establish a stable warning helper frame
    abi::emit_symbol_address(emitter, "rdi", "_diag_array_offset_on_null");
    emitter.instruction(&format!("mov esi, {}", ARRAY_OFFSET_ON_NULL_LEN));     // pass the complete array-offset-on-null warning length
    abi::emit_call_label(emitter, "__rt_diag_warning");                       // emit or suppress the PHP warning
    emitter.instruction("mov rsp, rbp");                                        // release the warning helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the null-receiver fallback
}

/// Emits the x86_64 implementation of `__rt_warn_undefined_array_key_int`.
fn emit_undefined_array_key_warning_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: undefined_array_key_warning ---");
    emitter.label_global("__rt_warn_undefined_array_key_int");

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable runtime warning frame
    emitter.instruction("sub rsp, 32");                                         // reserve saved key and concat cursor while keeping calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the missing integer key across warning fragments
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // snapshot concat scratch state before formatting the key
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the concat cursor across itoa

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_undefined_array_key_prefix");
    emitter.instruction(&format!("mov esi, {}", UNDEFINED_ARRAY_KEY_PREFIX_LEN)); // pass the undefined-key warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning prefix

    // -- emit formatted key --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the missing integer key for decimal formatting
    abi::emit_call_label(emitter, "__rt_itoa");                                 // format the missing key into concat scratch
    emitter.instruction("mov rdi, rax");                                        // pass the formatted missing-key pointer to the warning helper
    emitter.instruction("mov rsi, rdx");                                        // pass the formatted missing-key length to the warning helper
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the formatted missing-key value
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the pre-warning concat cursor
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);            // restore concat scratch state for surrounding expressions

    // -- emit suffix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_undefined_array_key_suffix");
    emitter.instruction(&format!("mov esi, {}", UNDEFINED_ARRAY_KEY_SUFFIX_LEN)); // pass the undefined-key warning suffix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning suffix

    // -- restore stack frame --
    emitter.instruction("mov rsp, rbp");                                        // release the runtime warning frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the array-miss caller
}

/// Emits the ARM64 implementation of `__rt_warn_undefined_array_key_str`.
fn emit_undefined_array_key_string_warning_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: undefined_array_key_string_warning ---");
    emitter.label_global("__rt_warn_undefined_array_key_str");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // reserve saved string key and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable runtime warning frame
    emitter.instruction("str x1, [sp, #0]");                                    // save the missing string key pointer across warning fragments
    emitter.instruction("str x2, [sp, #8]");                                    // save the missing string key length across warning fragments

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "x1", "_diag_undefined_array_key_prefix");
    emitter.instruction(&format!("mov x2, #{}", UNDEFINED_ARRAY_KEY_PREFIX_LEN)); // pass the undefined-key warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning prefix

    // -- emit quoted string key --
    abi::emit_symbol_address(emitter, "x1", "_diag_undefined_array_key_quote");
    emitter.instruction(&format!("mov x2, #{}", UNDEFINED_ARRAY_KEY_QUOTE_LEN)); // pass the opening quote length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the opening quote
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the missing string key pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the missing string key length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the missing string key bytes
    abi::emit_symbol_address(emitter, "x1", "_diag_undefined_array_key_quote");
    emitter.instruction(&format!("mov x2, #{}", UNDEFINED_ARRAY_KEY_QUOTE_LEN)); // pass the closing quote length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the closing quote

    // -- emit suffix --
    abi::emit_symbol_address(emitter, "x1", "_diag_undefined_array_key_suffix");
    emitter.instruction(&format!("mov x2, #{}", UNDEFINED_ARRAY_KEY_SUFFIX_LEN)); // pass the undefined-key warning suffix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning suffix

    // -- restore stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the runtime warning frame
    emitter.instruction("ret");                                                 // return to the array-miss caller
}

/// Emits the x86_64 implementation of `__rt_warn_undefined_array_key_str`.
fn emit_undefined_array_key_string_warning_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: undefined_array_key_string_warning ---");
    emitter.label_global("__rt_warn_undefined_array_key_str");

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable runtime warning frame
    emitter.instruction("sub rsp, 32");                                         // reserve saved key pointer and length while keeping calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the missing string key pointer across warning fragments
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the missing string key length across warning fragments

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_undefined_array_key_prefix");
    emitter.instruction(&format!("mov esi, {}", UNDEFINED_ARRAY_KEY_PREFIX_LEN)); // pass the undefined-key warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning prefix

    // -- emit quoted string key --
    abi::emit_symbol_address(emitter, "rdi", "_diag_undefined_array_key_quote");
    emitter.instruction(&format!("mov esi, {}", UNDEFINED_ARRAY_KEY_QUOTE_LEN)); // pass the opening quote length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the opening quote
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the missing string key pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the missing string key length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the missing string key bytes
    abi::emit_symbol_address(emitter, "rdi", "_diag_undefined_array_key_quote");
    emitter.instruction(&format!("mov esi, {}", UNDEFINED_ARRAY_KEY_QUOTE_LEN)); // pass the closing quote length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the closing quote

    // -- emit suffix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_undefined_array_key_suffix");
    emitter.instruction(&format!("mov esi, {}", UNDEFINED_ARRAY_KEY_SUFFIX_LEN)); // pass the undefined-key warning suffix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning suffix

    // -- restore stack frame --
    emitter.instruction("mov rsp, rbp");                                        // release the runtime warning frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the array-miss caller
}
