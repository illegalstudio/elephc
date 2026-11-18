//! Purpose:
//! Preserves compiler integration tests and imports for dense hash reindexing.
//!
//! Called from:
//! - Managed runtime emission and focused multi-target assembly tests.
//!
//! Key details:
//! - Physical ownership movement lives in Memoir while the compiler retains its adapter.

pub use elephc_memoir::values::hash_reindex_dense::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// The helper walks the source chain, moves entries under dense keys, clears the moved
    /// entries, and returns the new table on every supported architecture.
    #[test]
    fn reindex_emits_the_move_and_clear_sequence_on_every_target() {
        for (target, insert, done, clear) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "bl __rt_hash_insert_owned",
                "cmn x0, #1",
                "str xzr, [x9, #-24]",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "call __rt_hash_insert_owned",
                "cmp rax, -1",
                "mov QWORD PTR [r10 - 24], 0",
            ),
        ] {
            let mut emitter = Emitter::new(target);
            emit_hash_reindex_dense(&mut emitter);
            let output = emitter.output();
            for expected in [
                "__rt_hash_reindex_dense",
                "__rt_hash_iter_next",
                insert,
                done,
                clear,
            ] {
                assert!(
                    output.contains(expected),
                    "{target}: missing {expected}\n{output}"
                );
            }
        }
    }
}
