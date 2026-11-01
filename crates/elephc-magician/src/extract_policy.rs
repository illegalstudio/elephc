//! Purpose:
//! Shared `extract()` `EXTR_*` mode values and collision/prefix policy.
//!
//! Called from:
//! - `crate::ffi::extract::extract_inner` (the compiled backend's eval-bridge FFI, which is
//!   `cfg(not(test))` because it needs the real runtime bridge).
//! - `crate::interpreter::builtins::array::extract::eval_builtin_extract` (the tree-walking
//!   interpreter's own `extract()`, exercised directly by `cargo test`).
//!
//! Key details:
//! - Lives outside both call sites, and outside any `cfg(not(test))` gate, specifically so the
//!   two backends share the exact same mode numbers and the exact same collision/prefix decision
//!   instead of each re-deriving it -- a divergence here would be a silent behavioral difference
//!   between compiled and interpreted `extract()`.
//! - Depends only on `ElephcEvalScope` and the generic `RuntimeValueOps` trait, never on the
//!   concrete runtime-hook bridge, so it compiles under `cfg(test)` too.

use crate::errors::EvalStatus;
use crate::interpreter::RuntimeValueOps;
use crate::scope::ElephcEvalScope;
use crate::value::RuntimeCellHandle;

pub(crate) const EXTR_OVERWRITE: i64 = 0;
pub(crate) const EXTR_SKIP: i64 = 1;
pub(crate) const EXTR_PREFIX_SAME: i64 = 2;
pub(crate) const EXTR_PREFIX_ALL: i64 = 3;
pub(crate) const EXTR_PREFIX_INVALID: i64 = 4;
pub(crate) const EXTR_PREFIX_IF_EXISTS: i64 = 5;
pub(crate) const EXTR_IF_EXISTS: i64 = 6;
pub(crate) const EXTR_REFS: i64 = 256;

const EVAL_TAG_INT: u64 = 0;
const EVAL_TAG_STRING: u64 = 1;

/// Converts one foreach-visible array key into its PHP extraction name.
pub(crate) fn extract_key_name(
    values: &mut impl RuntimeValueOps,
    key: RuntimeCellHandle,
) -> Result<Option<String>, EvalStatus> {
    match values.type_tag(key)? {
        EVAL_TAG_STRING => String::from_utf8(values.string_bytes(key)?)
            .map(Some)
            .map_err(|_| EvalStatus::RuntimeFatal),
        EVAL_TAG_INT => Ok(Some((values.raw_value_word(key)? as i64).to_string())),
        _ => Ok(None),
    }
}

/// Applies collision/prefix policy and returns the final variable name when extractable.
pub(crate) fn extract_target_name(
    scope: &ElephcEvalScope,
    name: &str,
    prefix: &str,
    mode: i64,
) -> Option<String> {
    let valid = is_valid_php_variable_name(name);
    let exists = scope.contains_visible(name);
    let prefix_name = || format!("{prefix}_{name}");
    let target = match mode {
        EXTR_OVERWRITE if valid => name.to_string(),
        EXTR_SKIP if valid && !exists => name.to_string(),
        EXTR_PREFIX_SAME if valid && exists => prefix_name(),
        EXTR_PREFIX_SAME if valid => name.to_string(),
        EXTR_PREFIX_ALL if valid || is_prefixable_invalid_name(name) => prefix_name(),
        EXTR_PREFIX_INVALID if valid => name.to_string(),
        EXTR_PREFIX_INVALID if is_prefixable_invalid_name(name) => prefix_name(),
        EXTR_PREFIX_IF_EXISTS if valid && exists => prefix_name(),
        EXTR_IF_EXISTS if valid && exists => name.to_string(),
        _ => return None,
    };
    is_valid_php_variable_name(&target).then_some(target)
}

/// Returns whether bytes encoded as UTF-8 form a PHP variable identifier.
fn is_valid_php_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic() || !first.is_ascii())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric() || !ch.is_ascii())
}

/// Returns whether PHP can repair an invalid key by prefixing it.
fn is_prefixable_invalid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric() || !ch.is_ascii())
}
