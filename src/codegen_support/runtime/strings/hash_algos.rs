//! Purpose:
//! Emits the `__rt_hash_algos_list` runtime helper backing PHP's `hash_algos()`.
//! Builds a PHP array of the algorithm-name strings elephc-crypto supports.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - `HASH_ALGOS` is the single source of truth for the supported algorithm names
//!   and MUST stay in lockstep with `crates/elephc-crypto`'s `make()` table — every
//!   advertised name must be hashable by `hash()`. The labelled `.asciz` constants
//!   (`_hash_algo_N`) are emitted in `runtime::data::fixed` from this same list.
//! - Builds the result array with `__rt_array_new(len, 16)` + `__rt_array_push_str`,
//!   the same idiom as `__rt_str_split`. Returns the array handle (x0 / rax).

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// The hash algorithms elephc-crypto supports, in `hash_algos()` order. Subset of
/// PHP's `hash_algos()` (we omit gost/haval/snefru/tiger/murmur/xxh — documented
/// gaps). MUST match `crates/elephc-crypto/src/algos.rs`'s `make()` table.
pub(crate) const HASH_ALGOS: &[&str] = &[
    "md2", "md4", "md5", "sha1", "sha224", "sha256", "sha384", "sha512",
    "sha512/224", "sha512/256", "sha3-224", "sha3-256", "sha3-384", "sha3-512",
    "ripemd128", "ripemd160", "ripemd256", "ripemd320", "whirlpool", "crc32",
    "crc32b", "crc32c", "adler32", "fnv132", "fnv1a32", "fnv164", "fnv1a64", "joaat",
];

/// Emits the `__rt_hash_algos_list` helper for both targets.
///
/// Takes no arguments; returns a PHP array of the supported algorithm-name strings
/// (array handle in x0 on AArch64, rax on x86_64). Each name is a static `.asciz`
/// constant (`_hash_algo_N`) pushed via `__rt_array_push_str`.
pub fn emit_hash_algos_list(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_algos_list_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_algos (supported algorithm names) ---");
    emitter.label_global("__rt_hash_algos_list");
    emitter.instruction("sub sp, sp, #32");                                     // allocate a frame with a slot for the result array pointer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set the frame pointer
    emitter.instruction(&format!("mov x0, #{}", HASH_ALGOS.len()));             // initial capacity = number of supported algorithms
    emitter.instruction("mov x1, #16");                                         // elem_size = 16 (string ptr + len slots)
    abi::emit_call_label(emitter, "__rt_array_new");                            // allocate the result array
    emitter.instruction("str x0, [sp]");                                        // save the array pointer across the push loop
    for (i, name) in HASH_ALGOS.iter().enumerate() {
        abi::emit_symbol_address(emitter, "x1", &format!("_hash_algo_{}", i));
        emitter.instruction(&format!("mov x2, #{}", name.len()));               // algorithm-name byte length (excludes the .asciz NUL)
        emitter.instruction("ldr x0, [sp]");                                    // reload the array pointer before appending the name
        abi::emit_call_label(emitter, "__rt_array_push_str");                   // append the algorithm name as a string element
        emitter.instruction("str x0, [sp]");                                    // save the possibly-grown array pointer after the push
    }
    emitter.instruction("ldr x0, [sp]");                                        // return the completed array pointer
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the array handle
}

/// Emits the x86_64 Linux variant of the `__rt_hash_algos_list` helper.
fn emit_hash_algos_list_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_algos (supported algorithm names) ---");
    emitter.label_global("__rt_hash_algos_list");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the frame base
    emitter.instruction("sub rsp, 16");                                         // reserve an aligned slot for the result array pointer
    emitter.instruction(&format!("mov edi, {}", HASH_ALGOS.len()));             // initial capacity = number of supported algorithms
    emitter.instruction("mov esi, 16");                                         // elem_size = 16 (string ptr + len slots)
    abi::emit_call_label(emitter, "__rt_array_new");                            // allocate the result array
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the array pointer across the push loop
    for (i, name) in HASH_ALGOS.iter().enumerate() {
        abi::emit_symbol_address(emitter, "rsi", &format!("_hash_algo_{}", i));
        emitter.instruction(&format!("mov rdx, {}", name.len()));               // algorithm-name byte length (excludes the .asciz NUL)
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // reload the array pointer before appending the name
        abi::emit_call_label(emitter, "__rt_array_push_str");                   // append the algorithm name as a string element
        emitter.instruction("mov QWORD PTR [rbp - 8], rax");                    // save the possibly-grown array pointer after the push
    }
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the completed array pointer
    emitter.instruction("add rsp, 16");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the array handle
}
