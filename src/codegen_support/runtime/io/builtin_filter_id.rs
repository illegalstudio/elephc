//! Purpose:
//! Emits `__rt_builtin_filter_id`, which maps a run-time filter name to its
//! built-in stream-filter id.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `stream_filter_append` / `stream_filter_prepend` lowering when the filter
//!   name is not a compile-time literal.
//!
//! Key details:
//! - The lowering can only match a literal name against the built-in table, so
//!   `stream_filter_append($s, $name)` used to fall through to the user-filter
//!   path and silently attach nothing. PHP resolves built-ins by name whether it
//!   is a literal or not.
//! - The table is emitted as `(pointer, length, id)` triples so the matcher is a
//!   plain scan; ids match `stream_filter_id()` in the EIR lowering.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Built-in filter names paired with the ids the lowering already uses.
///
/// This table lists exactly the filters a CHAIN NODE can apply, because that is
/// what a resolved id becomes: `__rt_stream_apply_filter_chain` hands the id to
/// `__rt_apply_stream_filter`, a byte-transform helper. `zlib.*`, `bzip2.*` and
/// `convert.iconv.*` are not entries here and must not be added as such — they
/// are per-call-site attach sequences that install a per-fd handle and a
/// program-local helper thunk (`_zlib_fwrite_fn` and friends), which no
/// run-time name lookup can reconstruct. Naming them here would mint a node
/// carrying an id the chain then hands to a byte transform that does not exist
/// for deflate: the attach would report success and filter nothing.
///
/// `string.strip_tags` is absent for the opposite reason: php has no such
/// filter since 8.0, so it must miss and warn.
pub(crate) const BUILTIN_FILTER_NAMES: &[(&str, u8)] = &[
    ("string.toupper", 1),
    ("string.tolower", 2),
    ("string.rot13", 3),
    ("dechunk", 5),
    ("convert.base64-encode", 6),
    ("convert.base64-decode", 7),
    ("convert.quoted-printable-encode", 8),
    ("convert.quoted-printable-decode", 9),
    ("consumed", 13),
];

/// Emits the built-in filter name table into the runtime data section.
pub(crate) fn emit_builtin_filter_table(out: &mut String) {
    for (name, id) in BUILTIN_FILTER_NAMES {
        out.push_str(&format!(
            ".globl _bf_name_{id}\n_bf_name_{id}:\n    .ascii \"{name}\"\n"
        ));
    }
    out.push_str(".p2align 3\n.globl _bf_table\n_bf_table:\n");
    for (name, id) in BUILTIN_FILTER_NAMES {
        out.push_str(&format!(
            "    .quad _bf_name_{id}\n    .quad {}\n    .quad {id}\n",
            name.len()
        ));
    }
    out.push_str("    .quad 0\n    .quad 0\n    .quad 0\n");
}

/// `__rt_builtin_filter_id(ptr, len) -> id`, zero when the name is not built in.
///
/// Input:  AArch64 x0 = name pointer, x1 = name length.
///         x86_64  rdi = name pointer, rsi = name length.
pub fn emit_builtin_filter_id(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_builtin_filter_id_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: resolve a built-in stream filter name ---");
    emitter.label_global("__rt_builtin_filter_id");
    abi::emit_symbol_address(emitter, "x9", "_bf_table");
    emitter.label("__rt_bfid_entry");
    emitter.instruction("ldr x10, [x9]");                                       // candidate name pointer
    emitter.instruction("cbz x10, __rt_bfid_miss");                             // null pointer terminates the table
    emitter.instruction("ldr x11, [x9, #8]");                                   // candidate name length
    emitter.instruction("cmp x11, x1");                                         // does the length match?
    emitter.instruction("b.ne __rt_bfid_next");                                 // different length: try the next entry
    emitter.instruction("mov x12, #0");                                         // byte compare cursor
    emitter.label("__rt_bfid_bytes");
    emitter.instruction("cmp x12, x1");                                         // compared every byte?
    emitter.instruction("b.ge __rt_bfid_hit");                                  // full match
    emitter.instruction("ldrb w13, [x10, x12]");                                // candidate byte
    emitter.instruction("ldrb w14, [x0, x12]");                                 // requested byte
    emitter.instruction("cmp w13, w14");                                        // do the bytes match?
    emitter.instruction("b.ne __rt_bfid_next");                                 // mismatch: try the next entry
    emitter.instruction("add x12, x12, #1");                                    // advance the cursor
    emitter.instruction("b __rt_bfid_bytes");
    emitter.label("__rt_bfid_next");
    emitter.instruction("add x9, x9, #24");                                     // advance to the next 3-word entry
    emitter.instruction("b __rt_bfid_entry");
    emitter.label("__rt_bfid_hit");
    emitter.instruction("ldr x0, [x9, #16]");                                   // return the built-in filter id
    emitter.instruction("ret");
    emitter.label("__rt_bfid_miss");
    emitter.instruction("mov x0, #0");                                          // not a built-in filter name
    emitter.instruction("ret");
}

/// x86_64 variant of [`emit_builtin_filter_id`].
fn emit_builtin_filter_id_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve a built-in stream filter name ---");
    emitter.label_global("__rt_builtin_filter_id");
    abi::emit_symbol_address(emitter, "r9", "_bf_table");
    emitter.label("__rt_bfid_entry_x");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // candidate name pointer
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_bfid_miss_x");                                 // null pointer terminates the table
    emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                         // candidate name length
    emitter.instruction("cmp r11, rsi");                                        // does the length match?
    emitter.instruction("jne __rt_bfid_next_x");                                // different length: try the next entry
    emitter.instruction("xor rcx, rcx");                                        // byte compare cursor
    emitter.label("__rt_bfid_bytes_x");
    emitter.instruction("cmp rcx, rsi");                                        // compared every byte?
    emitter.instruction("jge __rt_bfid_hit_x");                                 // full match
    emitter.instruction("mov dl, BYTE PTR [r10 + rcx]");                        // candidate byte
    emitter.instruction("mov r8b, BYTE PTR [rdi + rcx]");                       // requested byte
    emitter.instruction("cmp dl, r8b");                                         // do the bytes match?
    emitter.instruction("jne __rt_bfid_next_x");                                // mismatch: try the next entry
    emitter.instruction("add rcx, 1");                                          // advance the cursor
    emitter.instruction("jmp __rt_bfid_bytes_x");
    emitter.label("__rt_bfid_next_x");
    emitter.instruction("add r9, 24");                                          // advance to the next 3-word entry
    emitter.instruction("jmp __rt_bfid_entry_x");
    emitter.label("__rt_bfid_hit_x");
    emitter.instruction("mov rax, QWORD PTR [r9 + 16]");                        // return the built-in filter id
    emitter.instruction("ret");
    emitter.label("__rt_bfid_miss_x");
    emitter.instruction("xor eax, eax");                                        // not a built-in filter name
    emitter.instruction("ret");
}
