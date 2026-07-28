//! Purpose:
//! Emits the `__rt_addr_tls_crypto_method` runtime helper, which reports whether a
//! `stream_socket_client()` address selects one of PHP's crypto transports.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - php-src registers `ssl`, `sslv3`, `tls` and `tlsv1.0`..`tlsv1.3` as crypto
//!   transports and sets `enable_on_connect` for each of them
//!   (`php_openssl_ssl_socket_factory`, ext/openssl/xp_ssl.c), so the handshake
//!   runs inside `stream_socket_client()`. `sslv2` is rejected by PHP outright and
//!   is deliberately not matched here.
//! - Scheme names are case-insensitive. Every byte of interest (letters, `1`, `.`,
//!   `:`, `/`) is unchanged or correctly folded by `| 0x20`, so one OR normalises
//!   the whole prefix and the comparison becomes a handful of 64-bit tests.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Little-endian encoding of `"ssl://"` in the low 48 bits.
const SSL_PREFIX: u64 = 0x00_00_2f_2f_3a_6c_73_73;
/// Little-endian encoding of `"tls://"` in the low 48 bits.
const TLS_PREFIX: u64 = 0x00_00_2f_2f_3a_73_6c_74;
/// Little-endian encoding of the eight bytes `"sslv3://"`.
const SSLV3_PREFIX: u64 = 0x2f_2f_3a_33_76_6c_73_73;
/// Little-endian encoding of the eight bytes `"tlsv1.N:"`, with `N` cleared.
const TLSV1_PREFIX_BASE: u64 = 0x3a_00_2e_31_76_73_6c_74;

/// Emits the `__rt_addr_tls_crypto_method` runtime helper.
///
/// # ABI (AArch64)
/// - Input: `x0` = address pointer, `x1` = address byte length
/// - Output: `x0` = 1 when the address selects a TLS transport, otherwise 0
///
/// # ABI (x86_64)
/// - Input: `rdi` = address pointer, `rsi` = address byte length
/// - Output: `rax` = 1 when the address selects a TLS transport, otherwise 0
pub fn emit_addr_tls_crypto_method(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_addr_tls_crypto_method_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // keep the entry aligned after preceding runtime literals
    emitter.comment("--- runtime: addr_is_tls_scheme ---");
    emitter.label_global("__rt_addr_tls_crypto_method");

    emitter.instruction("cmp x1, #6");                                          // shortest TLS address prefix is "ssl://"
    emitter.instruction("b.lo __rt_addr_tls_method_none");                      // too short to name a crypto transport
    emitter.instruction("ldrb w9, [x0]");                                       // assemble the first six bytes one at a time:
    emitter.instruction("ldrb w10, [x0, #1]");                                  // the address may be shorter than eight bytes, so a
    emitter.instruction("orr x9, x9, x10, lsl #8");                             // single 64-bit load could read past the string
    emitter.instruction("ldrb w10, [x0, #2]");                                  // (the runtime hands out exact-length slices)
    emitter.instruction("orr x9, x9, x10, lsl #16");                            // accumulate byte 2
    emitter.instruction("ldrb w10, [x0, #3]");                                  // load byte 3
    emitter.instruction("orr x9, x9, x10, lsl #24");                            // accumulate byte 3
    emitter.instruction("ldrb w10, [x0, #4]");                                  // load byte 4
    emitter.instruction("orr x9, x9, x10, lsl #32");                            // accumulate byte 4
    emitter.instruction("ldrb w10, [x0, #5]");                                  // load byte 5
    emitter.instruction("orr x9, x9, x10, lsl #40");                            // accumulate byte 5
    emitter.instruction("mov x11, #0x2020");                                    // build the ASCII case-folding mask
    emitter.instruction("movk x11, #0x2020, lsl #16");                          // scheme names are case-insensitive in PHP
    emitter.instruction("movk x11, #0x2020, lsl #32");                          // '.', ':' and '/' are unchanged by this OR
    emitter.instruction("orr x9, x9, x11");                                     // normalise the six-byte prefix to lower case
    emitter.instruction(&format!("mov x12, #{}", SSL_PREFIX & 0xFFFF));         // compare against "ssl://"
    emitter.instruction(&format!("movk x12, #{}, lsl #16", (SSL_PREFIX >> 16) & 0xFFFF)); // ...
    emitter.instruction(&format!("movk x12, #{}, lsl #32", (SSL_PREFIX >> 32) & 0xFFFF)); // ...
    emitter.instruction("cmp x9, x12");                                         // is the address an ssl:// address?
    emitter.instruction("b.eq __rt_addr_tls_method_any");                       // → TLS transport
    emitter.instruction(&format!("mov x12, #{}", TLS_PREFIX & 0xFFFF));         // compare against "tls://"
    emitter.instruction(&format!("movk x12, #{}, lsl #16", (TLS_PREFIX >> 16) & 0xFFFF)); // ...
    emitter.instruction(&format!("movk x12, #{}, lsl #32", (TLS_PREFIX >> 32) & 0xFFFF)); // ...
    emitter.instruction("cmp x9, x12");                                         // is the address a tls:// address?
    emitter.instruction("b.eq __rt_addr_tls_method_any");                       // → TLS transport

    // -- the remaining transports need eight normalised bytes --
    emitter.instruction("cmp x1, #8");                                          // "sslv3://" is exactly eight bytes
    emitter.instruction("b.lo __rt_addr_tls_method_none");                      // nothing longer can match
    emitter.instruction("ldrb w10, [x0, #6]");                                  // load byte 6
    emitter.instruction("orr x9, x9, x10, lsl #48");                            // accumulate byte 6
    emitter.instruction("ldrb w10, [x0, #7]");                                  // load byte 7
    emitter.instruction("orr x9, x9, x10, lsl #56");                            // accumulate byte 7
    emitter.instruction("mov x11, #0x2020");                                    // rebuild the fold mask for the full word
    emitter.instruction("movk x11, #0x2020, lsl #16");                          // ...
    emitter.instruction("movk x11, #0x2020, lsl #32");                          // ...
    emitter.instruction("movk x11, #0x2020, lsl #48");                          // ...
    emitter.instruction("orr x9, x9, x11");                                     // normalise the eight-byte prefix
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x12", SSLV3_PREFIX as i64); // "sslv3://"
    emitter.instruction("cmp x9, x12");                                         // is the address an sslv3:// address?
    emitter.instruction("b.eq __rt_addr_tls_method_sslv3");                     // → SSLv3-pinned transport

    // -- tlsv1.0:// .. tlsv1.3:// : the version digit is masked out before the compare --
    emitter.instruction("cmp x1, #10");                                         // "tlsv1.N://" is ten bytes
    emitter.instruction("b.lo __rt_addr_tls_method_none");                      // too short for a versioned transport
    emitter.instruction("ldrb w10, [x0, #8]");                                  // byte 8 must be '/'
    emitter.instruction("cmp w10, #0x2F");                                      // separator check
    emitter.instruction("b.ne __rt_addr_tls_method_none");                      // not a scheme separator
    emitter.instruction("ldrb w10, [x0, #9]");                                  // byte 9 must be '/'
    emitter.instruction("cmp w10, #0x2F");                                      // separator check
    emitter.instruction("b.ne __rt_addr_tls_method_none");                      // not a scheme separator
    emitter.instruction("ubfx x10, x9, #48, #8");                               // extract the version digit at byte 6
    emitter.instruction("sub x10, x10, #0x30");                                 // fold '0'..'3' to 0..3
    emitter.instruction("cmp x10, #3");                                         // reject any other digit
    emitter.instruction("b.hi __rt_addr_tls_method_none");                      // php only registers tlsv1.0..tlsv1.3
    emitter.instruction("and x9, x9, #0xFF00FFFFFFFFFFFF");                     // clear the version digit before comparing
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x12", TLSV1_PREFIX_BASE as i64); // "tlsv1.\0:"
    emitter.instruction("cmp x9, x12");                                         // is the address a tlsv1.N:// address?
    emitter.instruction("b.ne __rt_addr_tls_method_none");                      // → plain transport
    emitter.instruction("mov x0, #8");                                          // STREAM_CRYPTO_METHOD_TLSv1_N_CLIENT is 1 | (8 << N)
    emitter.instruction("lsl x0, x0, x10");                                     // shift by the version digit recovered above
    emitter.instruction("orr x0, x0, #1");                                      // set the client bit
    emitter.instruction("ret");                                                 // return the version-pinned crypto method

    emitter.label("__rt_addr_tls_method_any");
    emitter.instruction("mov x0, #121");                                        // STREAM_CRYPTO_METHOD_TLS_ANY_CLIENT (9|17|33|65)
    emitter.instruction("ret");                                                 // ssl:// and tls:// both default to it

    emitter.label("__rt_addr_tls_method_sslv3");
    emitter.instruction("mov x0, #5");                                          // STREAM_CRYPTO_METHOD_SSLv3_CLIENT
    emitter.instruction("ret");                                                 // the bridge rejects it, matching php without SSLv3

    emitter.label("__rt_addr_tls_method_none");
    emitter.instruction("mov x0, #0");                                          // plain transport, no handshake on connect
    emitter.instruction("ret");                                                 // return "not a crypto transport"
}

/// Emits the x86_64 implementation of `__rt_addr_tls_crypto_method`.
///
/// Same algorithm as the AArch64 path; see `emit_addr_tls_crypto_method`.
fn emit_addr_tls_crypto_method_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: addr_is_tls_scheme ---");
    emitter.label_global("__rt_addr_tls_crypto_method");

    emitter.instruction("cmp rsi, 6");                                          // shortest TLS address prefix is "ssl://"
    emitter.instruction("jb __rt_addr_tls_method_none_x86");                    // too short to name a crypto transport
    emitter.instruction("xor r9, r9");                                          // accumulate the prefix one byte at a time so a
    emitter.instruction("movzx r10d, BYTE PTR [rdi]");                          // short address is never read past its end
    emitter.instruction("or r9, r10");                                          // accumulate byte 0
    emitter.instruction("movzx r10d, BYTE PTR [rdi + 1]");                      // load byte 1
    emitter.instruction("shl r10, 8");                                          // position byte 1
    emitter.instruction("or r9, r10");                                          // accumulate byte 1
    emitter.instruction("movzx r10d, BYTE PTR [rdi + 2]");                      // load byte 2
    emitter.instruction("shl r10, 16");                                         // position byte 2
    emitter.instruction("or r9, r10");                                          // accumulate byte 2
    emitter.instruction("movzx r10d, BYTE PTR [rdi + 3]");                      // load byte 3
    emitter.instruction("shl r10, 24");                                         // position byte 3
    emitter.instruction("or r9, r10");                                          // accumulate byte 3
    emitter.instruction("movzx r10d, BYTE PTR [rdi + 4]");                      // load byte 4
    emitter.instruction("shl r10, 32");                                         // position byte 4
    emitter.instruction("or r9, r10");                                          // accumulate byte 4
    emitter.instruction("movzx r10d, BYTE PTR [rdi + 5]");                      // load byte 5
    emitter.instruction("shl r10, 40");                                         // position byte 5
    emitter.instruction("or r9, r10");                                          // accumulate byte 5
    emitter.instruction("mov r11, 0x202020202020");                             // ASCII case-folding mask for six bytes
    emitter.instruction("or r9, r11");                                          // normalise the prefix to lower case
    emitter.instruction(&format!("mov r11, {}", SSL_PREFIX));                   // compare against "ssl://"
    emitter.instruction("cmp r9, r11");                                         // is the address an ssl:// address?
    emitter.instruction("je __rt_addr_tls_method_any_x86");                     // → TLS transport
    emitter.instruction(&format!("mov r11, {}", TLS_PREFIX));                   // compare against "tls://"
    emitter.instruction("cmp r9, r11");                                         // is the address a tls:// address?
    emitter.instruction("je __rt_addr_tls_method_any_x86");                     // → TLS transport

    // -- the remaining transports need eight normalised bytes --
    emitter.instruction("cmp rsi, 8");                                          // "sslv3://" is exactly eight bytes
    emitter.instruction("jb __rt_addr_tls_method_none_x86");                    // nothing longer can match
    emitter.instruction("movzx r10d, BYTE PTR [rdi + 6]");                      // load byte 6
    emitter.instruction("shl r10, 48");                                         // position byte 6
    emitter.instruction("or r9, r10");                                          // accumulate byte 6
    emitter.instruction("movzx r10d, BYTE PTR [rdi + 7]");                      // load byte 7
    emitter.instruction("shl r10, 56");                                         // position byte 7
    emitter.instruction("or r9, r10");                                          // accumulate byte 7
    emitter.instruction("mov r11, 0x2020202020202020");                         // fold mask for the full eight-byte word
    emitter.instruction("or r9, r11");                                          // normalise the eight-byte prefix
    emitter.instruction(&format!("mov r11, {}", SSLV3_PREFIX));                 // "sslv3://"
    emitter.instruction("cmp r9, r11");                                         // is the address an sslv3:// address?
    emitter.instruction("je __rt_addr_tls_method_sslv3_x86");                   // → SSLv3-pinned transport

    // -- tlsv1.0:// .. tlsv1.3:// : the version digit is masked out before the compare --
    emitter.instruction("cmp rsi, 10");                                         // "tlsv1.N://" is ten bytes
    emitter.instruction("jb __rt_addr_tls_method_none_x86");                    // too short for a versioned transport
    emitter.instruction("cmp BYTE PTR [rdi + 8], 0x2F");                        // byte 8 must be '/'
    emitter.instruction("jne __rt_addr_tls_method_none_x86");                   // not a scheme separator
    emitter.instruction("cmp BYTE PTR [rdi + 9], 0x2F");                        // byte 9 must be '/'
    emitter.instruction("jne __rt_addr_tls_method_none_x86");                   // not a scheme separator
    emitter.instruction("mov r10, r9");                                         // copy the word to extract the version digit
    emitter.instruction("shr r10, 48");                                         // move byte 6 into the low bits
    emitter.instruction("movzx r10d, r10b");                                    // isolate the digit
    emitter.instruction("sub r10, 0x30");                                       // fold '0'..'3' to 0..3
    emitter.instruction("cmp r10, 3");                                          // reject any other digit
    emitter.instruction("ja __rt_addr_tls_method_none_x86");                    // php only registers tlsv1.0..tlsv1.3
    emitter.instruction("mov r11, 0xFF00FFFFFFFFFFFF");                         // mask clearing the version digit
    emitter.instruction("and r9, r11");                                         // clear the digit before comparing
    emitter.instruction(&format!("mov r11, {}", TLSV1_PREFIX_BASE));            // "tlsv1.\0:"
    emitter.instruction("cmp r9, r11");                                         // is the address a tlsv1.N:// address?
    emitter.instruction("jne __rt_addr_tls_method_none_x86");                   // → plain transport
    emitter.instruction("mov ecx, r10d");                                       // the shift count must live in cl
    emitter.instruction("mov eax, 8");                                          // STREAM_CRYPTO_METHOD_TLSv1_N_CLIENT is 1 | (8 << N)
    emitter.instruction("shl eax, cl");                                         // shift by the version digit recovered above
    emitter.instruction("or eax, 1");                                           // set the client bit
    emitter.instruction("ret");                                                 // return the version-pinned crypto method

    emitter.label("__rt_addr_tls_method_any_x86");
    emitter.instruction("mov eax, 121");                                        // STREAM_CRYPTO_METHOD_TLS_ANY_CLIENT (9|17|33|65)
    emitter.instruction("ret");                                                 // ssl:// and tls:// both default to it

    emitter.label("__rt_addr_tls_method_sslv3_x86");
    emitter.instruction("mov eax, 5");                                          // STREAM_CRYPTO_METHOD_SSLv3_CLIENT
    emitter.instruction("ret");                                                 // the bridge rejects it, matching php without SSLv3

    emitter.label("__rt_addr_tls_method_none_x86");
    emitter.instruction("xor eax, eax");                                        // plain transport, no handshake on connect
    emitter.instruction("ret");                                                 // return "not a crypto transport"
}
