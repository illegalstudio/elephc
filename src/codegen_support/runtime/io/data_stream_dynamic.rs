//! Purpose:
//! Emits `__rt_data_stream_dynamic`, which opens an RFC 2397 `data://` URI whose bytes are only
//! known at run time.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The dynamic `fopen()` lowering, when the path carries the `data://` prefix.
//!
//! Key details:
//! - A literal `data://` URI is decoded during lowering and its bytes embedded, which is why a
//!   run-time URI had no path at all and answered `false`.
//! - Decoding needs nothing new: `__rt_base64_decode` and `__rt_urldecode` already exist and both
//!   take and return elephc's string pair. `__rt_urldecode` also maps `+` to a space, which is
//!   exactly what the compile-time decoder does for these URIs, so the two agree.
//! - `__rt_data_uri_meta_ok` decides whether php-src would accept the media type at all, and
//!   whether it asks for base64. php-src is stricter than a `;base64` suffix test: the type is
//!   either empty or must carry a `/`, every parameter must be `name=value`, and `base64`
//!   counts only as the LAST parameter and only in lower case.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_data_stream_dynamic`.
pub fn emit_data_stream_dynamic(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_meta_ok_aarch64(emitter);
            emit_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_meta_ok_x86_64(emitter);
            emit_x86_64(emitter);
        }
    }
}

/// `__rt_data_stream_dynamic(x0 = uri, x1 = length) -> x0 = descriptor, or -1`.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: open a run-time data:// URI ---");
    emitter.label_global("__rt_data_stream_dynamic");
    // Frame: [0]=cursor [8]=remaining [16]=comma offset [24]=uri ptr [32]=uri len
    //        [40]=reason ptr [48]=reason len [64]/[72]=linkage.
    //
    // The URI is kept WHOLE from entry because every refusal below names it: php prints
    // `fopen(<the uri>): Failed to open stream: <reason>`, and by the time a refusal is reached
    // the cursor has been advanced past the scheme.
    emitter.instruction("sub sp, sp, #80");                                     // reserve the decode frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #24]");                                   // the URI, whole, for the refusal lines
    emitter.instruction("str x1, [sp, #32]");
    emitter.instruction("cmp x1, #5");                                          // "data:" is the whole scheme
    emitter.instruction("b.lt __rt_dsd_no_comma");                              // too short to carry a comma either
    emitter.instruction("add x0, x0, #5");                                      // step past "data:"
    emitter.instruction("sub x1, x1, #5");
    // php-src's `php_stream_locate_url_wrapper` special-cases this ONE scheme: a wrapper normally
    // needs `://`, but the test is `!strncmp("//", p+1, 2) || (n == 4 && !memcmp("data:", path, 5))`
    // — so `data:text/plain,hi` opens exactly like `data://text/plain,hi`, and the `//` is optional
    // rather than part of the scheme. Measured: both answer `'hi'` on `php -n` 8.5.6, while elephc
    // sent the slash-less form to the FILE opener and reported "No such file or directory".
    emitter.instruction("cmp x1, #2");
    emitter.instruction("b.lt __rt_dsd_after_slashes");                         // nothing left to be the "//"
    emitter.instruction("ldrb w9, [x0]");
    emitter.instruction("cmp w9, #47");                                         // ASCII '/'
    emitter.instruction("b.ne __rt_dsd_after_slashes");
    emitter.instruction("ldrb w9, [x0, #1]");
    emitter.instruction("cmp w9, #47");
    emitter.instruction("b.ne __rt_dsd_after_slashes");
    emitter.instruction("add x0, x0, #2");                                      // the optional "//" is not media type
    emitter.instruction("sub x1, x1, #2");
    emitter.label("__rt_dsd_after_slashes");
    emitter.instruction("str x0, [sp, #0]");                                    // the media-type cursor
    emitter.instruction("str x1, [sp, #8]");

    // -- the comma separates the media type from the payload --
    emitter.instruction("mov x9, #0");                                          // scan index
    emitter.label("__rt_dsd_comma");
    emitter.instruction("cmp x9, x1");                                          // ran off the end?
    emitter.instruction("b.hs __rt_dsd_no_comma");                              // php names this one exactly
    emitter.instruction("ldrb w10, [x0, x9]");
    emitter.instruction("cmp w10, #44");                                        // ASCII ','
    emitter.instruction("b.eq __rt_dsd_split");                                 // found the separator
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("b __rt_dsd_comma");

    emitter.label("__rt_dsd_split");
    emitter.instruction("str x9, [sp, #16]");                                   // the media type's length

    // -- validate the media type and learn whether it asks for base64 --
    emitter.instruction("ldr x0, [sp, #0]");                                    // the media type
    emitter.instruction("mov x1, x9");                                          // its length
    emitter.instruction("bl __rt_data_uri_meta_ok");                            // 0 bad type, 1 plain, 2 base64, 3 bad parameter
    emitter.instruction("cbz x0, __rt_dsd_bad_media");                          // the type segment carries no '/'
    emitter.instruction("cmp x0, #3");
    emitter.instruction("b.eq __rt_dsd_bad_param");                             // a parameter segment carries no '='
    emitter.instruction("cmp x0, #2");
    emitter.instruction("b.eq __rt_dsd_base64");                                // the last parameter was `base64`
    emitter.instruction("b __rt_dsd_percent");                                  // otherwise the payload is percent-encoded

    emitter.label("__rt_dsd_base64");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the media-type cursor
    emitter.instruction("ldr x1, [sp, #8]");                                    // bytes after the scheme
    emitter.instruction("ldr x9, [sp, #16]");                                   // the media type's length
    emitter.instruction("add x2, x0, x9");                                      // the comma
    emitter.instruction("add x1, x2, #1");                                      // the payload starts after it
    emitter.instruction("ldr x2, [sp, #8]");
    emitter.instruction("sub x2, x2, x9");                                      // bytes from the comma on
    emitter.instruction("sub x2, x2, #1");                                      // minus the comma itself
    // php-src decodes a data URI with `php_base64_decode_ex(..., /* strict */ 1)` and answers
    // NULL — not an empty string — when the payload is not base64. This asked for the LAX mode
    // (and asked for it in the wrong register: the flag is x3, so what it actually passed was
    // whatever the caller left there), so `data://text/plain;base64,!!!bad!!!` opened a stream
    // over the lax decoder's salvage instead of failing. Measured: php answers false there, and
    // still decodes `SGVs bG8=` — strict mode drops whitespace, it only refuses stray bytes.
    emitter.instruction("mov x3, #1");                                          // strict, as php-src is here
    emitter.instruction("bl __rt_base64_decode");                               // x0 = 0 when strict mode refused
    emitter.instruction("cbz x0, __rt_dsd_undecodable");                        // php names this one exactly
    emitter.instruction("b __rt_dsd_open");

    emitter.label("__rt_dsd_percent");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the media-type cursor
    emitter.instruction("ldr x9, [sp, #16]");                                   // the media type's length
    emitter.instruction("add x1, x0, x9");                                      // the comma
    emitter.instruction("add x1, x1, #1");                                      // the payload starts after it
    emitter.instruction("ldr x2, [sp, #8]");
    emitter.instruction("sub x2, x2, x9");                                      // bytes from the comma on
    emitter.instruction("sub x2, x2, #1");                                      // minus the comma itself
    emitter.instruction("bl __rt_urldecode");                                   // x1/x2 = the decoded payload

    emitter.label("__rt_dsd_open");
    emitter.instruction("mov x0, x1");                                          // the decoded bytes
    emitter.instruction("mov x1, x2");                                          // and their length
    emitter.instruction("bl __rt_data_stream");                                 // x0 = the descriptor
    emitter.instruction("ldp x29, x30, [sp, #64]");
    emitter.instruction("add sp, sp, #80");
    emitter.instruction("ret");

    // -- the four sentences php-src refuses a data URI with, each naming the whole URI --
    emitter.label("__rt_dsd_no_comma");
    emit_refusal_reason_aarch64(emitter, "_diag_rfc2397_no_comma", "rfc2397: no comma in URL");
    emitter.label("__rt_dsd_undecodable");
    emit_refusal_reason_aarch64(emitter, "_diag_rfc2397_undecodable", "rfc2397: unable to decode");
    emitter.label("__rt_dsd_bad_media");
    emit_refusal_reason_aarch64(emitter, "_diag_rfc2397_media_type", "rfc2397: illegal media type");
    emitter.label("__rt_dsd_bad_param");
    emit_refusal_reason_aarch64(emitter, "_diag_rfc2397_parameter", "rfc2397: illegal parameter");

    emitter.label("__rt_dsd_no");
    // The composer wants a NUL-terminated path, and the URI arrived as a pointer/length pair, so
    // it goes through the same `__rt_cstr` scratch `__rt_fopen` uses. The composer copies out of
    // that scratch before anything else can claim it.
    emitter.instruction("ldr x1, [sp, #24]");                                   // the URI, still whole
    emitter.instruction("ldr x2, [sp, #32]");
    emitter.instruction("bl __rt_cstr");                                        // x0 = a NUL-terminated copy
    emitter.instruction("mov x2, x0");                                          // the path php names in the parentheses
    emitter.instruction("ldr x3, [sp, #40]");                                   // the reason this refusal chose
    emitter.instruction("ldr x4, [sp, #48]");
    abi::emit_symbol_address(emitter, "x0", "_diag_open_failed_fopen_prefix");
    emitter.instruction(&format!("mov x1, #{}", "Warning: fopen(".len()));
    emitter.instruction("bl __rt_open_failed_reason_warning");
    emitter.instruction("mov x0, #-1");                                         // an unusable data URI opens nothing
    emitter.instruction("ldp x29, x30, [sp, #64]");
    emitter.instruction("add sp, sp, #80");
    emitter.instruction("ret");
}

/// Parks one refusal reason in the frame and joins the shared warn-and-fail tail.
fn emit_refusal_reason_aarch64(emitter: &mut Emitter, symbol: &str, reason: &str) {
    abi::emit_symbol_address(emitter, "x9", symbol);
    emitter.instruction("str x9, [sp, #40]");
    emitter.instruction(&format!("mov x9, #{}", reason.len()));
    emitter.instruction("str x9, [sp, #48]");
    emitter.instruction("b __rt_dsd_no");
}

/// x86_64 form of [`emit_aarch64`].
///
/// `__rt_data_stream_dynamic(rdi = uri, rsi = length) -> rax = descriptor, or -1`.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: open a run-time data:// URI ---");
    emitter.label_global("__rt_data_stream_dynamic");
    // Frame: [rbp-8]=cursor [rbp-16]=remaining [rbp-24]=comma offset [rbp-32]=uri ptr
    //        [rbp-40]=uri len [rbp-48]=reason ptr [rbp-56]=reason len.
    //
    // See the AArch64 counterpart on why the URI is kept whole.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the decode frame
    emitter.instruction("sub rsp, 64");                                         // reserve the spill slots
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // the URI, whole, for the refusal lines
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");
    emitter.instruction("cmp rsi, 5");                                          // "data:" is the whole scheme
    emitter.instruction("jl __rt_dsd_no_comma_x");                              // too short to carry a comma either
    emitter.instruction("add rdi, 5");                                          // step past "data:"
    emitter.instruction("sub rsi, 5");
    // See the AArch64 counterpart: php-src makes the `//` optional for this ONE scheme.
    emitter.instruction("cmp rsi, 2");
    emitter.instruction("jl __rt_dsd_after_slashes_x");                         // nothing left to be the "//"
    emitter.instruction("movzx eax, BYTE PTR [rdi]");
    emitter.instruction("cmp eax, 47");                                         // ASCII '/'
    emitter.instruction("jne __rt_dsd_after_slashes_x");
    emitter.instruction("movzx eax, BYTE PTR [rdi + 1]");
    emitter.instruction("cmp eax, 47");
    emitter.instruction("jne __rt_dsd_after_slashes_x");
    emitter.instruction("add rdi, 2");                                          // the optional "//" is not media type
    emitter.instruction("sub rsi, 2");
    emitter.label("__rt_dsd_after_slashes_x");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the media-type cursor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");

    emitter.instruction("xor r9, r9");                                          // scan index
    emitter.label("__rt_dsd_comma_x");
    emitter.instruction("cmp r9, rsi");                                         // ran off the end?
    emitter.instruction("jae __rt_dsd_no_comma_x");                             // php names this one exactly
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");
    emitter.instruction("cmp eax, 44");                                         // ASCII ','
    emitter.instruction("je __rt_dsd_split_x");                                 // found the separator
    emitter.instruction("add r9, 1");
    emitter.instruction("jmp __rt_dsd_comma_x");

    emitter.label("__rt_dsd_split_x");
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // the media type's length

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the media type
    emitter.instruction("mov rsi, r9");                                         // its length
    emitter.instruction("call __rt_data_uri_meta_ok");                          // 0 bad type, 1 plain, 2 base64, 3 bad parameter
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_dsd_bad_media_x");                             // the type segment carries no '/'
    emitter.instruction("cmp rax, 3");
    emitter.instruction("je __rt_dsd_bad_param_x");                             // a parameter segment carries no '='
    emitter.instruction("cmp rax, 2");
    emitter.instruction("je __rt_dsd_base64_x");                                // the last parameter was `base64`
    emitter.instruction("jmp __rt_dsd_percent_x");                              // otherwise percent-encoded

    emitter.label("__rt_dsd_base64_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the media-type cursor
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // the media type's length
    emitter.instruction("lea rax, [rdi + r9]");                                 // the comma
    emitter.instruction("add rax, 1");                                          // the payload starts after it
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");
    emitter.instruction("sub rdx, r9");                                         // bytes from the comma on
    emitter.instruction("sub rdx, 1");                                          // minus the comma itself
    // See the AArch64 counterpart: php-src decodes a data URI STRICTLY and answers NULL, not an
    // empty string, when the payload is not base64.
    emitter.instruction("mov edi, 1");                                          // strict, as php-src is here
    emitter.instruction("call __rt_base64_decode");                             // rax/rdx payload, r8 = 0 when strict mode refused
    emitter.instruction("test r8, r8");
    emitter.instruction("jz __rt_dsd_undecodable_x");                           // php names this one exactly
    emitter.instruction("jmp __rt_dsd_open_x");

    emitter.label("__rt_dsd_percent_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the media-type cursor
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // the media type's length
    emitter.instruction("lea rax, [rdi + r9]");                                 // the comma
    emitter.instruction("add rax, 1");                                          // the payload starts after it
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");
    emitter.instruction("sub rdx, r9");                                         // bytes from the comma on
    emitter.instruction("sub rdx, 1");                                          // minus the comma itself
    emitter.instruction("call __rt_urldecode");                                 // rax/rdx = the decoded payload

    emitter.label("__rt_dsd_open_x");
    emitter.instruction("mov rdi, rax");                                        // the decoded bytes
    emitter.instruction("mov rsi, rdx");                                        // and their length
    emitter.instruction("call __rt_data_stream");                               // rax = the descriptor
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");

    // -- the four sentences php-src refuses a data URI with, each naming the whole URI --
    emitter.label("__rt_dsd_no_comma_x");
    emit_refusal_reason_x86_64(emitter, "_diag_rfc2397_no_comma", "rfc2397: no comma in URL");
    emitter.label("__rt_dsd_undecodable_x");
    emit_refusal_reason_x86_64(emitter, "_diag_rfc2397_undecodable", "rfc2397: unable to decode");
    emitter.label("__rt_dsd_bad_media_x");
    emit_refusal_reason_x86_64(emitter, "_diag_rfc2397_media_type", "rfc2397: illegal media type");
    emitter.label("__rt_dsd_bad_param_x");
    emit_refusal_reason_x86_64(emitter, "_diag_rfc2397_parameter", "rfc2397: illegal parameter");

    emitter.label("__rt_dsd_no_x");
    // See the AArch64 counterpart on the `__rt_cstr` round trip.
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // the URI, still whole
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");
    emitter.instruction("call __rt_cstr");                                      // rax = a NUL-terminated copy
    emitter.instruction("mov rdx, rax");                                        // the path php names in the parentheses
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // the reason this refusal chose
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");
    abi::emit_symbol_address(emitter, "rdi", "_diag_open_failed_fopen_prefix");
    emitter.instruction(&format!("mov esi, {}", "Warning: fopen(".len()));
    emitter.instruction("call __rt_open_failed_reason_warning");
    emitter.instruction("mov rax, -1");                                         // an unusable data URI opens nothing
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// The x86_64 counterpart of [`emit_refusal_reason_aarch64`].
fn emit_refusal_reason_x86_64(emitter: &mut Emitter, symbol: &str, reason: &str) {
    abi::emit_symbol_address(emitter, "r9", symbol);
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");
    emitter.instruction(&format!("mov QWORD PTR [rbp - 56], {}", reason.len()));
    emitter.instruction("jmp __rt_dsd_no_x");
}

/// `__rt_data_uri_meta_ok(x0 = media type, x1 = length) -> x0 = 0 invalid, 1 plain, 2 base64`.
///
/// One pass over the media type, closing a segment at every `;` and at the end. The first segment
/// is the type and must be empty or carry a `/`; every later one is a parameter and must carry an
/// `=`, unless it is the final `base64`. `data_uri_media_type_shape` applies the same rule to a
/// literal URI at compile time — neither can serve both, so they are pinned by one fixture.
fn emit_meta_ok_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is a data:// media type one php-src accepts ---");
    emitter.label_global("__rt_data_uri_meta_ok");
    emitter.instruction("sub sp, sp, #48");                                     // frame for the segment scan
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("cbz x1, __rt_dumo_plain");                             // an empty media type is accepted
    emitter.instruction("str x0, [sp, #0]");                                    // the media type
    emitter.instruction("str x1, [sp, #8]");                                    // its length
    emitter.instruction("mov x9, #0");                                          // cursor
    emitter.instruction("mov x10, #0");                                         // current segment start
    emitter.instruction("mov x11, #0");                                         // segment index
    emitter.instruction("mov x12, #0");                                         // saw a '/' in this segment
    emitter.instruction("mov x13, #0");                                         // saw an '=' in this segment

    emitter.label("__rt_dumo_byte");
    emitter.instruction("cmp x9, x1");                                          // reached the end?
    emitter.instruction("b.eq __rt_dumo_close");                                // close the final segment
    emitter.instruction("ldrb w14, [x0, x9]");
    emitter.instruction("cmp w14, #59");                                        // ASCII ';'
    emitter.instruction("b.eq __rt_dumo_close");                                // close this segment
    emitter.instruction("cmp w14, #47");                                        // ASCII '/'
    emitter.instruction("b.ne __rt_dumo_eq");
    emitter.instruction("mov x12, #1");                                         // the type carries a slash
    emitter.label("__rt_dumo_eq");
    emitter.instruction("cmp w14, #61");                                        // ASCII '='
    emitter.instruction("b.ne __rt_dumo_next");
    emitter.instruction("mov x13, #1");                                         // the parameter carries an '='
    emitter.label("__rt_dumo_next");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("b __rt_dumo_byte");

    emitter.label("__rt_dumo_close");
    emitter.instruction("cbnz x11, __rt_dumo_param");                           // segments after the first are parameters
    // -- the type: empty is fine, otherwise it must carry a slash --
    emitter.instruction("cmp x9, x10");                                         // is the type empty?
    emitter.instruction("b.eq __rt_dumo_advance");                              // an empty type is accepted
    emitter.instruction("cbz x12, __rt_dumo_bad");                              // a type without '/' is refused
    emitter.instruction("b __rt_dumo_advance");

    emitter.label("__rt_dumo_param");
    // -- `base64` is accepted only as the LAST parameter --
    emitter.instruction("sub x14, x9, x10");                                    // this parameter's length
    emitter.instruction("cmp x14, #6");
    emitter.instruction("b.ne __rt_dumo_param_eq");                             // not the right length for "base64"
    emitter.instruction("cmp x9, x1");                                          // is it the last segment?
    emitter.instruction("b.ne __rt_dumo_param_eq");                             // a later parameter follows, so it is not the marker
    emitter.instruction("str x9, [sp, #16]");                                   // preserve the cursor across the compare
    emitter.instruction("str x10, [sp, #24]");
    emitter.instruction("add x0, x0, x10");                                     // the parameter's bytes
    emitter.instruction("mov x1, #6");
    abi::emit_symbol_address(emitter, "x2", "_data_n_b64");
    emitter.instruction("add x2, x2, #1");                                      // skip the ';' in ";base64"
    emitter.instruction("mov x3, #6");
    emitter.instruction("bl __rt_pf_match");                                    // does it spell base64 exactly?
    emitter.instruction("cbnz x0, __rt_dumo_base64");                           // it does: the payload is base64
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the media type
    emitter.instruction("ldr x1, [sp, #8]");
    emitter.instruction("ldr x9, [sp, #16]");
    emitter.instruction("ldr x10, [sp, #24]");
    emitter.label("__rt_dumo_param_eq");
    emitter.instruction("cbz x13, __rt_dumo_bad_param");                        // a parameter without '=' is refused

    emitter.label("__rt_dumo_advance");
    emitter.instruction("cmp x9, x1");                                          // was that the final segment?
    emitter.instruction("b.eq __rt_dumo_plain");                                // the whole media type is well formed
    emitter.instruction("add x9, x9, #1");                                      // step past the ';'
    emitter.instruction("mov x10, x9");                                         // the next segment starts here
    emitter.instruction("add x11, x11, #1");                                    // count the segment
    emitter.instruction("mov x12, #0");                                         // reset the per-segment markers
    emitter.instruction("mov x13, #0");
    emitter.instruction("b __rt_dumo_byte");

    emitter.label("__rt_dumo_plain");
    emitter.instruction("mov x0, #1");                                          // accepted, percent-encoded payload
    emitter.instruction("b __rt_dumo_done");
    emitter.label("__rt_dumo_base64");
    emitter.instruction("mov x0, #2");                                          // accepted, base64 payload
    emitter.instruction("b __rt_dumo_done");
    emitter.label("__rt_dumo_bad");
    emitter.instruction("mov x0, #0");                                          // refused: the TYPE segment carries no '/'
    emitter.instruction("b __rt_dumo_done");
    emitter.label("__rt_dumo_bad_param");
    emitter.instruction("mov x0, #3");                                          // refused: a PARAMETER segment carries no '='
    emitter.label("__rt_dumo_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
}

/// x86_64 form of [`emit_meta_ok_aarch64`].
///
/// `__rt_data_uri_meta_ok(rdi = media type, rsi = length) -> rax = 0 invalid, 1 plain, 2 base64`.
fn emit_meta_ok_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: is a data:// media type one php-src accepts ---");
    emitter.label_global("__rt_data_uri_meta_ok");
    // Frame: [rbp-8]=media type [rbp-16]=length [rbp-24]=cursor [rbp-32]=segment start
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 48");
    emitter.instruction("test rsi, rsi");
    emitter.instruction("jz __rt_dumo_plain_x");                                // an empty media type is accepted
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the media type
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // its length
    emitter.instruction("xor r9, r9");                                          // cursor
    emitter.instruction("xor r10, r10");                                        // current segment start
    emitter.instruction("xor r11, r11");                                        // segment index
    emitter.instruction("xor rcx, rcx");                                        // saw a '/' in this segment
    emitter.instruction("xor rdx, rdx");                                        // saw an '=' in this segment

    emitter.label("__rt_dumo_byte_x");
    emitter.instruction("cmp r9, rsi");                                         // reached the end?
    emitter.instruction("je __rt_dumo_close_x");                                // close the final segment
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");
    emitter.instruction("cmp eax, 59");                                         // ASCII ';'
    emitter.instruction("je __rt_dumo_close_x");                                // close this segment
    emitter.instruction("cmp eax, 47");                                         // ASCII '/'
    emitter.instruction("jne __rt_dumo_eq_x");
    emitter.instruction("mov rcx, 1");                                          // the type carries a slash
    emitter.label("__rt_dumo_eq_x");
    emitter.instruction("cmp eax, 61");                                         // ASCII '='
    emitter.instruction("jne __rt_dumo_next_x");
    emitter.instruction("mov rdx, 1");                                          // the parameter carries an '='
    emitter.label("__rt_dumo_next_x");
    emitter.instruction("add r9, 1");
    emitter.instruction("jmp __rt_dumo_byte_x");

    emitter.label("__rt_dumo_close_x");
    emitter.instruction("test r11, r11");
    emitter.instruction("jnz __rt_dumo_param_x");                               // segments after the first are parameters
    emitter.instruction("cmp r9, r10");                                         // is the type empty?
    emitter.instruction("je __rt_dumo_advance_x");                              // an empty type is accepted
    emitter.instruction("test rcx, rcx");
    emitter.instruction("jz __rt_dumo_bad_x");                                  // a type without '/' is refused
    emitter.instruction("jmp __rt_dumo_advance_x");

    emitter.label("__rt_dumo_param_x");
    emitter.instruction("mov rax, r9");
    emitter.instruction("sub rax, r10");                                        // this parameter's length
    emitter.instruction("cmp rax, 6");
    emitter.instruction("jne __rt_dumo_param_eq_x");                            // not the right length for "base64"
    emitter.instruction("cmp r9, rsi");                                         // is it the last segment?
    emitter.instruction("jne __rt_dumo_param_eq_x");                            // a later parameter follows
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // preserve the scan across the compare
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");
    emitter.instruction("add rdi, r10");                                        // the parameter's bytes
    emitter.instruction("mov rsi, 6");
    abi::emit_symbol_address(emitter, "rdx", "_data_n_b64");
    emitter.instruction("add rdx, 1");                                          // skip the ';' in ";base64"
    emitter.instruction("mov rcx, 6");
    emitter.instruction("call __rt_pf_match");                                  // does it spell base64 exactly?
    emitter.instruction("test rax, rax");
    emitter.instruction("jnz __rt_dumo_base64_x");                              // it does: the payload is base64
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the media type
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");
    emitter.instruction("xor rdx, rdx");                                        // the compare clobbered the '=' marker
    emitter.label("__rt_dumo_param_eq_x");
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_dumo_bad_param_x");                            // a parameter without '=' is refused

    emitter.label("__rt_dumo_advance_x");
    emitter.instruction("cmp r9, rsi");                                         // was that the final segment?
    emitter.instruction("je __rt_dumo_plain_x");                                // the whole media type is well formed
    emitter.instruction("add r9, 1");                                           // step past the ';'
    emitter.instruction("mov r10, r9");                                         // the next segment starts here
    emitter.instruction("add r11, 1");                                          // count the segment
    emitter.instruction("xor rcx, rcx");                                        // reset the per-segment markers
    emitter.instruction("xor rdx, rdx");
    emitter.instruction("jmp __rt_dumo_byte_x");

    emitter.label("__rt_dumo_plain_x");
    emitter.instruction("mov rax, 1");                                          // accepted, percent-encoded payload
    emitter.instruction("jmp __rt_dumo_done_x");
    emitter.label("__rt_dumo_base64_x");
    emitter.instruction("mov rax, 2");                                          // accepted, base64 payload
    emitter.instruction("jmp __rt_dumo_done_x");
    emitter.label("__rt_dumo_bad_x");
    emitter.instruction("xor eax, eax");                                        // refused: the TYPE segment carries no '/'
    emitter.instruction("jmp __rt_dumo_done_x");
    emitter.label("__rt_dumo_bad_param_x");
    emitter.instruction("mov rax, 3");                                          // refused: a PARAMETER segment carries no '='
    emitter.label("__rt_dumo_done_x");
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}
