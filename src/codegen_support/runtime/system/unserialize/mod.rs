//! Purpose:
//! Emits the `__rt_unserialize_mixed` runtime helper (and its internal cursor-based
//! recursive parser `__rt_unser_at` / key parser `__rt_unser_key`) that parse a PHP
//! `serialize()` wire string into a freshly boxed Mixed value.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//! - The EIR `unserialize()` lowering in `crate::codegen_support::lower_inst::builtins::system`.
//!
//! Key details:
//! - Recognizes scalars `N;`, `b:0;`/`b:1;`, `i:<int>;`, `d:<float>;` (including
//!   `INF`/`-INF`/`NAN`), `s:<bytelen>:"<raw>";`, arrays, objects, and references.
//!   Objects resolve declared classes and invoke supported hydration hooks; references
//!   reuse entries from the per-call registry.
//! - Arrays build a hash (`__rt_hash_new` value_type 7) whose values are boxed Mixed
//!   cells stored with per-entry tag 7, the canonical heterogeneous representation.
//!   Scalar/value boxes come from `__rt_mixed_from_value` and transfer into the hash;
//!   string keys are borrowed and persisted by `__rt_hash_set`.
//! - Manually boxed arrays and objects on x86_64 preserve the runtime heap marker in
//!   the upper half of their allocation-kind word.
//! - Blocked objects retain their persisted original class name and a Mixed property
//!   hash so `__PHP_Incomplete_Class` reserializes with correct reference rebasing.
//! - Begin/end helpers isolate reentrant calls by snapshotting policy, parser depth,
//!   and the used reference-registry prefix. Option values are normalized into an
//!   owned direct-string array before hydration hooks can observe them.
//! - The allocation-free preflight validates every cursor, overflow, delimiter,
//!   child, key, and closing brace before the mutating parser allocates or calls hooks.
//!   Floats call `strtod` only after a bounded semicolon scan and verify its end pointer.
//! - Emission order is part of the generated assembly contract and remains fixed by
//!   this orchestrator across the target-specific module split.

mod allowed_classes_aarch64;
mod allowed_classes_x86_64;
mod context;
mod date_magic_restore;
mod decoder_aarch64;
mod decoder_x86_64;
mod diagnostics;
mod storage_aarch64;
mod storage_x86_64;
mod validator_aarch64;
mod validator_x86_64;

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_unserialize_mixed` and every helper required by the selected target.
///
/// The public helper accepts a serialized string and returns a boxed Mixed pointer,
/// or zero for malformed or unsupported wire data so the caller can produce PHP
/// `false`. Its target ABI is documented by the decoder entry module.
pub(crate) fn emit_unserialize(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => allowed_classes_aarch64::emit(emitter),
        Arch::X86_64 => allowed_classes_x86_64::emit(emitter),
    }

    diagnostics::emit_unserialize_type_error_helper(emitter);
    diagnostics::emit_unserialize_object_string_error_helper(emitter);
    diagnostics::emit_unserialize_object_to_string_helper(emitter);
    date_magic_restore::emit(emitter);

    match emitter.target.arch {
        Arch::AArch64 => {
            decoder_aarch64::emit_entry(emitter);
            validator_aarch64::emit(emitter);
            decoder_aarch64::emit_parser(emitter);
            context::emit_unserialize_context_aarch64(emitter);
            storage_aarch64::emit_object_storage(emitter);
            storage_aarch64::emit_key(emitter);
        }
        Arch::X86_64 => {
            decoder_x86_64::emit_entry(emitter);
            validator_x86_64::emit(emitter);
            decoder_x86_64::emit_parser(emitter);
            context::emit_unserialize_context_x86_64(emitter);
            storage_x86_64::emit_object_storage(emitter);
            storage_x86_64::emit_key(emitter);
        }
    }
}
