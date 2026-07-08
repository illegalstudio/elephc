//! Purpose:
//! Type-checks the io PHP builtin family.
//! Validates arity, argument types, warning-producing cases, and inferred return types for direct calls.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin()`
//!
//! Key details:
//! - Signatures, callable aliases, optimizer effects, and codegen builtin dispatch must remain in lockstep.
//! - The `stats` submodule has been fully migrated to the builtin registry (io batch B) and is deleted.
//! - The `files` submodule (`__elephc_phar_*` intrinsics) has been fully migrated to the builtin
//!   registry (io batch C2) and is deleted; these now live in `src/builtins/io/__elephc_phar_*.rs`.
//! - The `streams` submodule (stream socket/network builtins) has been fully migrated to the builtin
//!   registry (io batch G) and is deleted; all io builtins now live in `src/builtins/io/`.

pub(crate) mod common;
