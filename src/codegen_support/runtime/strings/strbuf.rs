//! Purpose:
//! Exposes compiler imports and tests for Memoir storage assembly.
//!
//! Called from:
//! - Compiler runtime orchestration and focused memory integration tests.
//!
//! Key details:
//! - Preserves the compiler namespace while Memoir owns the assembly implementation.

pub use elephc_memoir::strings::strbuf::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// A slice written in place at the cursor is committed before the append grows the buffer:
    /// growth copies only the bytes below the cursor, so growing first would drop or free the
    /// in-place bytes. Both ABIs test the cursor, then commit, then ensure the headroom.
    #[test]
    fn strbuf_append_commits_an_in_place_slice_before_growing() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_strbuf(&mut emitter);
            let asm = emitter.output();
            let append = asm.split("__rt_strbuf_append:").nth(1).unwrap();
            let (cursor_test, committed) = match target.arch {
                Arch::AArch64 => ("cmp x1, x9", "add x9, x9, x2"),
                Arch::X86_64 => ("cmp rsi, r9", "add r9, rdx"),
            };
            let in_place = append.find(cursor_test).unwrap();
            let commit = append.find(committed).unwrap();
            let grow = append.find("__rt_strbuf_ensure").unwrap();
            assert!(in_place < commit && commit < grow, "{:?}", target.arch);
        }
    }

    /// Growth must go through the shared transient-block helper on both ABIs, and an appended
    /// child slice must be released only when it is a transient block, so a JSON container never
    /// frees a literal, a scratch slice, or a string somebody else owns.
    #[test]
    fn strbuf_grows_through_concat_grow_and_releases_only_transient_sources() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_strbuf(&mut emitter);
            let asm = emitter.output();
            for label in [
                "__rt_strbuf_init:",
                "__rt_strbuf_ensure:",
                "__rt_strbuf_sync:",
                "__rt_strbuf_finish:",
                "__rt_strbuf_finish_owned:",
                "__rt_strbuf_release:",
                "__rt_strbuf_append:",
                "__rt_strbuf_reserve_temp:",
            ] {
                assert!(asm.contains(label), "{label} missing on {:?}", target.arch);
            }
            assert_eq!(asm.matches("__rt_concat_grow").count(), 1, "{:?}", target.arch);
            let append = asm.split("__rt_strbuf_append:").nth(1).unwrap();
            let kind = append.find("__rt_heap_kind").unwrap();
            let free = append.find("__rt_heap_free").unwrap();
            assert!(kind < free, "the kind probe guards the release on {:?}", target.arch);
        }
    }
}
