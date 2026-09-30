//! Purpose:
//! Emits `__rt_filter_absorb_params` and `__rt_asf_params_load`, which carry the built-in stream
//! filter parameters from a PHP `$params` array to the encoders that honour them.
//!
//! Called from:
//! - `crate::codegen_support::runtime::resources::filter`, once per attach and once per buffer.
//! - `crate::codegen_support::runtime::io::{fread, fwrite}`, which reset them on the legacy
//!   per-descriptor path.
//!
//! Key details:
//! - php's built-in filters DO read `$params`. `convert.base64-encode` and
//!   `convert.quoted-printable-encode` take `line-length` and `line-break-chars`, and the
//!   quoted-printable pair also takes `binary`. elephc ignored the argument entirely, so
//!   `stream_filter_append($h, "convert.base64-encode", STREAM_FILTER_WRITE, ["line-length" => 8])`
//!   produced one unbroken line where php produces wrapped ones.
//! - The array is read ONCE, at attach, into four plain words on the filter node. Reading it per
//!   buffer would put a hash probe in the encoder's inner path for a value that cannot change:
//!   php parses `$params` in the filter's `create` callback for the same reason.
//! - The parsed values reach the encoders through globals rather than a fifth argument, because
//!   `__rt_apply_stream_filter` is a leaf with no frame and its scratch registers are already
//!   spoken for. `__rt_asf_params_load` publishes them; every caller invokes it, including the
//!   legacy per-descriptor path, which passes 0 so a previous chain's `line-length` cannot leak
//!   into an unrelated read.
//! - Defaults are what a zeroed node already says. Measured on `php -n` 8.5.6, neither encoder
//!   wraps unless a `line-length` is named, so 0 means "no wrapping" and needs no sentinel.

use crate::codegen_support::runtime::resources::layout::{
    FILTER_BINARY_OFFSET, FILTER_BREAK_LEN_OFFSET, FILTER_BREAK_PTR_OFFSET,
    FILTER_BUILTIN_ID_OFFSET, FILTER_LINE_LENGTH_OFFSET, FILTER_PARAMS_OFFSET,
};

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// `convert.base64-encode`, the first built-in filter id that PARSES `$params`.
///
/// The four `convert.*` ids are contiguous — 6, 7, 8, 9 — which is what lets the refusal be a
/// range test rather than four compares. Every other built-in accepts any `$params` because it
/// never reads it.
const FIRST_PARAM_PARSING_FILTER_ID: u64 = 6;

/// `convert.quoted-printable-decode`, the last such id.
const LAST_PARAM_PARSING_FILTER_ID: u64 = 9;

/// Emits `__rt_filter_absorb_params(state) -> ok`, which parses the node's retained `$params`.
///
/// Reads `line-length`, `line-break-chars` and `binary` and publishes them as plain words on the
/// node. A missing KEY leaves the zeroed default in place, which is php's behaviour.
///
/// Answers 0 when the node's filter PARSES `$params` and was handed something that is not an
/// array. Only the four `convert.*` filters parse it — measured on `php -n` 8.5.6, `string.*`,
/// `dechunk`, `zlib.*` and `bzip2.*` accept a null, an int or a string without complaint, because
/// they never look at it. The caller turns a 0 into php's two warnings and a `false`.
pub fn emit_filter_absorb_params(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_absorb_aarch64(emitter),
        Arch::X86_64 => emit_absorb_x86_64(emitter),
    }
}

/// Emits `__rt_asf_params_load(state_or_zero)`, which publishes a node's parameters for the
/// encoders, or clears them when there is no node.
pub fn emit_asf_params_load(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_params_load_aarch64(emitter),
        Arch::X86_64 => emit_params_load_x86_64(emitter),
    }
}

/// The AArch64 absorber.
fn emit_absorb_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: absorb built-in filter params ---");
    emitter.label_global("__rt_filter_absorb_params");
    // Frame: [0]=state [8]=hash.
    emitter.instruction("sub sp, sp, #32");                                     // reserve the absorber frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // the node whose fields we fill
    emitter.instruction("cbz x0, __rt_fap_done");                               // no node, nothing to fill
    emitter.instruction(&format!("ldr x0, [x0, #{FILTER_PARAMS_OFFSET}]"));     // the retained `$params` box
    emitter.instruction("cbz x0, __rt_fap_done");                               // the argument was not supplied at all
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0 = tag, x1 = payload
    emitter.instruction("cmp x0, #5");                                          // runtime tag 5 identifies a hash
    emitter.instruction("b.eq __rt_fap_array");
    emitter.instruction("cmp x0, #4");                                          // tag 4 identifies a packed array
    emitter.instruction("b.eq __rt_fap_done");                                  // an array with no string keys: nothing to read
    // -- not an array: only the filters that PARSE `$params` refuse it --
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction(&format!("ldr x9, [x9, #{FILTER_BUILTIN_ID_OFFSET}]")); // which built-in this node runs
    emitter.instruction(&format!("cmp x9, #{FIRST_PARAM_PARSING_FILTER_ID}"));  // the four `convert.*` ids are contiguous
    emitter.instruction("b.lo __rt_fap_done");                                  // every other filter ignores `$params`
    emitter.instruction(&format!("cmp x9, #{LAST_PARAM_PARSING_FILTER_ID}"));
    emitter.instruction("b.hi __rt_fap_done");
    emitter.instruction("mov x0, #0");                                          // php refuses the attach outright
    emitter.instruction("ldp x29, x30, [sp, #16]");
    emitter.instruction("add sp, sp, #32");
    emitter.instruction("ret");
    emitter.label("__rt_fap_array");
    emitter.instruction("str x1, [sp, #8]");                                    // the hash every lookup below reads

    // -- line-length --
    emitter.instruction("ldr x0, [sp, #8]");
    abi::emit_symbol_address(emitter, "x1", "_filter_key_line_length");
    emitter.instruction("mov x2, #11");                                         // "line-length"
    emitter.instruction("bl __rt_hash_get");
    emitter.instruction("cbz x0, __rt_fap_no_length");                          // key absent: no wrapping
    emitter.instruction("cmp x3, #0");                                          // runtime tag 0 identifies an integer
    emitter.instruction("b.ne __rt_fap_no_length");                             // php reads this one as an int
    emitter.instruction("cmp x1, #0");
    emitter.instruction("b.lt __rt_fap_no_length");                             // a negative length wraps at nothing
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction(&format!("str x1, [x9, #{FILTER_LINE_LENGTH_OFFSET}]"));
    emitter.label("__rt_fap_no_length");

    // -- line-break-chars --
    emitter.instruction("ldr x0, [sp, #8]");
    abi::emit_symbol_address(emitter, "x1", "_filter_key_line_break");
    emitter.instruction("mov x2, #16");                                         // "line-break-chars"
    emitter.instruction("bl __rt_hash_get");
    emitter.instruction("cbz x0, __rt_fap_no_break");                           // key absent: the default `\n` stands
    emitter.instruction("cmp x3, #1");                                          // runtime tag 1 identifies a string
    emitter.instruction("b.ne __rt_fap_no_break");
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction(&format!("str x1, [x9, #{FILTER_BREAK_PTR_OFFSET}]"));  // the caller's own bytes
    emitter.instruction(&format!("str x2, [x9, #{FILTER_BREAK_LEN_OFFSET}]"));  // and their length
    emitter.label("__rt_fap_no_break");

    // -- binary --
    emitter.instruction("ldr x0, [sp, #8]");
    abi::emit_symbol_address(emitter, "x1", "_filter_key_binary");
    emitter.instruction("mov x2, #6");                                          // "binary"
    emitter.instruction("bl __rt_hash_get");
    emitter.instruction("cbz x0, __rt_fap_done");                               // key absent: php's text rules
    emitter.instruction("cbz x1, __rt_fap_done");                               // a falsy value is not the binary mode
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction("mov x10, #1");
    emitter.instruction(&format!("str x10, [x9, #{FILTER_BINARY_OFFSET}]"));

    emitter.label("__rt_fap_done");
    emitter.instruction("mov x0, #1");                                          // the parameters were usable
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the absorber frame
    emitter.instruction("ret");
}

/// The x86_64 absorber.
fn emit_absorb_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: absorb built-in filter params ---");
    emitter.label_global("__rt_filter_absorb_params");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 32");                                         // reserve the node and hash slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the node whose fields we fill
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_fap_done_x86");                                // no node, nothing to fill
    emitter.instruction(&format!(
        "mov rdi, QWORD PTR [rdi + {FILTER_PARAMS_OFFSET}]"
    ));                                                                         // the retained `$params` box
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_fap_done_x86");                                // the argument was not supplied at all
    // `__rt_mixed_unbox` reads its box from RAX on this target, not rdi. Passing it in rdi
    // unboxed whatever rax happened to hold — the node pointer its own caller left there — so the
    // tag was never 4 or 5 and the four `convert.*` filters refused an ARRAY they must accept.
    // MEASURED in CI: `linux-x86_64` answered `arr=false` where php and aarch64 answer `arr=true`.
    emitter.instruction("mov rax, rdi");                                        // the box the helper reads
    emitter.instruction("call __rt_mixed_unbox");                               // rax = tag, rdi = payload
    emitter.instruction("cmp rax, 5");                                          // runtime tag 5 identifies a hash
    emitter.instruction("je __rt_fap_array_x86");
    emitter.instruction("cmp rax, 4");                                          // tag 4 identifies a packed array
    emitter.instruction("je __rt_fap_done_x86");                                // an array with no string keys
    // -- not an array: only the filters that PARSE `$params` refuse it --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [r10 + {FILTER_BUILTIN_ID_OFFSET}]"
    ));                                                                         // which built-in this node runs
    emitter.instruction(&format!("cmp r10, {FIRST_PARAM_PARSING_FILTER_ID}"));  // the four `convert.*` ids are contiguous
    emitter.instruction("jb __rt_fap_done_x86");                                // every other filter ignores `$params`
    emitter.instruction(&format!("cmp r10, {LAST_PARAM_PARSING_FILTER_ID}"));
    emitter.instruction("ja __rt_fap_done_x86");
    emitter.instruction("xor eax, eax");                                        // php refuses the attach outright
    emitter.instruction("add rsp, 32");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
    emitter.label("__rt_fap_array_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // the hash every lookup below reads

    // -- line-length --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    abi::emit_symbol_address(emitter, "rsi", "_filter_key_line_length");
    emitter.instruction("mov rdx, 11");                                         // "line-length"
    emitter.instruction("call __rt_hash_get");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fap_no_length_x86");                           // key absent: no wrapping
    emitter.instruction("cmp rcx, 0");                                          // runtime tag 0 identifies an integer
    emitter.instruction("jne __rt_fap_no_length_x86");                          // php reads this one as an int
    emitter.instruction("cmp rdi, 0");
    emitter.instruction("jl __rt_fap_no_length_x86");                           // a negative length wraps at nothing
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {FILTER_LINE_LENGTH_OFFSET}], rdi"
    ));
    emitter.label("__rt_fap_no_length_x86");

    // -- line-break-chars --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    abi::emit_symbol_address(emitter, "rsi", "_filter_key_line_break");
    emitter.instruction("mov rdx, 16");                                         // "line-break-chars"
    emitter.instruction("call __rt_hash_get");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fap_no_break_x86");                            // key absent: the default `\n` stands
    emitter.instruction("cmp rcx, 1");                                          // runtime tag 1 identifies a string
    emitter.instruction("jne __rt_fap_no_break_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {FILTER_BREAK_PTR_OFFSET}], rdi"
    ));                                                                         // the caller's own bytes
    emitter.instruction(&format!(
        "mov QWORD PTR [r10 + {FILTER_BREAK_LEN_OFFSET}], rsi"
    ));                                                                         // and their length
    emitter.label("__rt_fap_no_break_x86");

    // -- binary --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    abi::emit_symbol_address(emitter, "rsi", "_filter_key_binary");
    emitter.instruction("mov rdx, 6");                                          // "binary"
    emitter.instruction("call __rt_hash_get");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_fap_done_x86");                                // key absent: php's text rules
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_fap_done_x86");                                // a falsy value is not the binary mode
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction(&format!("mov QWORD PTR [r10 + {FILTER_BINARY_OFFSET}], 1"));

    emitter.label("__rt_fap_done_x86");
    emitter.instruction("mov rax, 1");                                          // the parameters were usable
    emitter.instruction("add rsp, 32");                                         // release the absorber frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}

/// The AArch64 publisher.
fn emit_params_load_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: publish built-in filter params for the encoders ---");
    emitter.label_global("__rt_asf_params_load");
    emitter.instruction("mov x9, #0");                                          // the cleared defaults
    emitter.instruction("mov x10, #0");
    emitter.instruction("mov x11, #0");
    emitter.instruction("mov x12, #0");
    emitter.instruction("cbz x0, __rt_asfpl_publish");                          // no node: the legacy path filters unparameterized
    emitter.instruction(&format!("ldr x9, [x0, #{FILTER_LINE_LENGTH_OFFSET}]"));
    emitter.instruction(&format!("ldr x10, [x0, #{FILTER_BREAK_PTR_OFFSET}]"));
    emitter.instruction(&format!("ldr x11, [x0, #{FILTER_BREAK_LEN_OFFSET}]"));
    emitter.instruction(&format!("ldr x12, [x0, #{FILTER_BINARY_OFFSET}]"));
    emitter.label("__rt_asfpl_publish");
    abi::emit_symbol_address(emitter, "x13", "_asf_line_length");
    emitter.instruction("str x9, [x13]");
    abi::emit_symbol_address(emitter, "x13", "_asf_break_ptr");
    emitter.instruction("str x10, [x13]");
    abi::emit_symbol_address(emitter, "x13", "_asf_break_len");
    emitter.instruction("str x11, [x13]");
    abi::emit_symbol_address(emitter, "x13", "_asf_binary");
    emitter.instruction("str x12, [x13]");
    emitter.instruction("ret");
}

/// The x86_64 publisher.
fn emit_params_load_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: publish built-in filter params for the encoders ---");
    emitter.label_global("__rt_asf_params_load");
    emitter.instruction("xor r8d, r8d");                                        // the cleared defaults
    emitter.instruction("xor r9d, r9d");
    emitter.instruction("xor r10d, r10d");
    emitter.instruction("xor r11d, r11d");
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_asfpl_publish_x86");                           // no node: the legacy path filters unparameterized
    emitter.instruction(&format!(
        "mov r8, QWORD PTR [rdi + {FILTER_LINE_LENGTH_OFFSET}]"
    ));
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rdi + {FILTER_BREAK_PTR_OFFSET}]"
    ));
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rdi + {FILTER_BREAK_LEN_OFFSET}]"
    ));
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rdi + {FILTER_BINARY_OFFSET}]"
    ));
    emitter.label("__rt_asfpl_publish_x86");
    abi::emit_symbol_address(emitter, "rax", "_asf_line_length");
    emitter.instruction("mov QWORD PTR [rax], r8");
    abi::emit_symbol_address(emitter, "rax", "_asf_break_ptr");
    emitter.instruction("mov QWORD PTR [rax], r9");
    abi::emit_symbol_address(emitter, "rax", "_asf_break_len");
    emitter.instruction("mov QWORD PTR [rax], r10");
    abi::emit_symbol_address(emitter, "rax", "_asf_binary");
    emitter.instruction("mov QWORD PTR [rax], r11");
    emitter.instruction("ret");
}
