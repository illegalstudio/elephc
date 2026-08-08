//! Purpose:
//! Owns restricted templates and basic status/JIT derivation.
//!
//! Called from:
//! - The OPcache prelude facade and sibling rendering modules.
//!
//! Key details:
//! - The unavailable-JIT clamp preserves configured mode metadata.

#[allow(unused_imports)]
use super::*;

/// Synthetic baseline of OPcache shared memory reported in-use for a freshly started
/// cache (0 cached scripts). The absolute figure is implementation-defined; only the
/// invariant `free_memory = memory_consumption - used_memory - wasted_memory` (with
/// `wasted_memory = 0`) is guaranteed exact. 6 MiB is a modest plausible baseline.
pub(super) const STATUS_USED_MEMORY: i64 = 6_291_456;

/// Synthetic baseline of the interned-strings buffer reported in-use, for the DEFAULT 8 MiB
/// buffer. The invariant `free_memory = buffer_size - used_memory` is guaranteed exact; the
/// absolute figure is implementation-defined (reference PHP 8.5.6 reports 2659216 of 8388608 on
/// this host, with 11679 strings, and no two builds agree). 1 MiB is a modest plausible baseline.
///
/// It is a CEILING, not a constant: see [`render_interned_used_memory`] for the small-buffer
/// scaling that keeps `used_memory < buffer_size` and `free_memory > 0`.
pub(super) const STATUS_INTERNED_USED_MEMORY: i64 = 1_048_576;

/// Synthetic plausible count of interned strings for a freshly started cache. Absolute
/// figure is implementation-defined.
pub(super) const STATUS_INTERNED_NUMBER_OF_STRINGS: i64 = 4_096;

/// `interned_strings_buffer` is reported in MiB by the directive table but as a byte
/// count in the status array; this is the MiB→byte factor.
pub(super) const BYTES_PER_MIB: i64 = 1_048_576;

/// Renders the `interned_strings_usage.used_memory` figure for a buffer of `buffer_size` bytes.
///
/// TWO HARD INVARIANTS, both taken from reference PHP's own arithmetic
/// (`used = top - base`, `free = end - top`, `buffer = end - base`, with `base <= top < end`):
/// `0 < used_memory < buffer_size` and therefore `free_memory > 0`. They are what make the figures
/// COHERENT rather than merely plausible, and they are exactly what the old flat constant broke:
/// with `--ini opcache.interned_strings_buffer=1` the 1 MiB baseline equalled the whole 1 MiB
/// buffer and elephc reported `free_memory => 0`, which reference PHP never does. VERIFIED on
/// reference PHP 8.5.6 with `-d opcache.interned_strings_buffer=1`:
/// `buffer_size 1048576, used_memory 824200, free_memory 224376` — used strictly below buffer.
///
/// The rule is `min(baseline, buffer_size / 2)`: the 1 MiB baseline for every buffer of 2 MiB or
/// more (so the DEFAULT 8 MiB rendering is byte-identical to what it was before this function
/// existed), and half the buffer below that. A buffer of 0 never reaches here — the whole key is
/// omitted (see [`render_interned_strings_usage`]) — so the result is always at least 1 byte for
/// any buffer of 2 bytes or more; the sub-2-byte buffers `opcache.interned_strings_buffer` can
/// never express (its unit is the MiB) are the only inputs that would degenerate.
pub(super) fn render_interned_used_memory(buffer_size: i64) -> i64 {
    STATUS_INTERNED_USED_MEMORY.min(buffer_size / 2)
}

/// Looks up an integer-valued `opcache.*` directive for `version_id`, with any `--ini`
/// overrides applied (`effective_opcache_directives`). The byte-verified table always carries
/// these keys as integers, and an integer directive's override only ever parses to an integer,
/// so a miss or type mismatch is a compiler bug and panics.
pub(super) fn directive_int(version_id: u32, key: &str, overrides: &[(String, String)]) -> i64 {
    effective_opcache_directives(version_id, overrides)
        .into_iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| match value {
            DirectiveValue::Int(int) => int,
            _ => panic!("opcache directive `{key}` must be an integer"),
        })
        .unwrap_or_else(|| panic!("opcache directive `{key}` must exist"))
}

/// Looks up a string-valued `opcache.*` directive for `version_id`, with any `--ini` overrides
/// applied. The byte-verified table always carries `opcache.jit` as a string, and a string
/// directive's override only ever parses to a string, so a miss or type mismatch panics.
pub(super) fn directive_str(version_id: u32, key: &str, overrides: &[(String, String)]) -> String {
    effective_opcache_directives(version_id, overrides)
        .into_iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| match value {
            DirectiveValue::Str(string) => string.to_string(),
            _ => panic!("opcache directive `{key}` must be a string"),
        })
        .unwrap_or_else(|| panic!("opcache directive `{key}` must exist"))
}

/// One `jit` sub-array's baked scalar fields, derived from the `opcache.jit` directive for the
/// compile target (see [`render_jit_status`] for the always-unavailable clamp on the other four).
pub(super) struct JitStatus {
    pub(super) enabled: bool,
    pub(super) on: bool,
    pub(super) kind: i64,
    pub(super) opt_level: i64,
    pub(super) opt_flags: i64,
    pub(super) buffer_size: i64,
    pub(super) buffer_free: i64,
}

/// Derives the `jit` sub-array of `opcache_get_status()` from the target's `opcache.jit`
/// directive (plus any `--ini` override), using the FULL reference directive → status mapping
/// for `kind` / `opt_level` / `opt_flags` (`crate::opcache::directives::effective_jit_config`,
/// which also models what an invalid spelling does), and then applying ONE clamp:
///
/// > **`enabled = false`, `on = false`, `buffer_size = 0`, `buffer_free = 0` — ALWAYS**,
/// > whatever `opcache.jit` says.
///
/// WHY THIS IS THE FAITHFUL CHOICE, not a shortcut. Reference PHP emits exactly this shape
/// itself whenever the JIT is CONFIGURED BUT UNAVAILABLE IN THIS PROCESS: it keeps reporting the
/// configured `kind`/`opt_level`/`opt_flags` (that is what was asked for) while reporting
/// `enabled`/`on` false and both buffer figures 0 (nothing was actually stood up). An elephc
/// binary is ahead-of-time compiled native code with no runtime JIT engine and no JIT buffer, so
/// "configured but unavailable" is not an approximation of its situation — it IS its situation.
/// Reporting `enabled = true` would be the divergence, since no caller could then trust
/// `$s['jit']['enabled']` as a "will my code be JIT-compiled?" probe.
///
/// THE REFERENCE EVIDENCE (re-verified on this host, PHP 8.5.6 and 8.2.31 Homebrew, macOS arm64
/// — all three are byte-identical apart from the version-dependent `kind`/`opt_level`/`opt_flags`):
/// - 8.5.6, JIT unavailable because Xdebug overrides `zend_execute_ex`, with
///   `-d opcache.jit=tracing -d opcache.jit_buffer_size=64M` →
///   `enabled=false, on=false, kind=5, opt_level=4, opt_flags=6, buffer_size=0, buffer_free=0`
///   (plus a startup `Warning: JIT is incompatible with third party extensions…`).
/// - 8.5.6, no Xdebug, JIT unavailable because there is no buffer, with
///   `-d opcache.jit=tracing -d opcache.jit_buffer_size=0` → the IDENTICAL array, silently.
/// - 8.2.31 with its DEFAULT `opcache.jit=tracing` and Xdebug loaded → the identical array
///   again, which is exactly what an elephc 8.2/8.3 target now renders with no `--ini` at all.
///
/// For contrast, the same 8.5.6 with the JIT genuinely available reports
/// `enabled=true, on=true, kind=5, opt_level=4, opt_flags=6, buffer_size=67108848,
/// buffer_free=67105256` — the shape elephc must NOT claim.
///
/// PER-VERSION CONSEQUENCE: on an 8.4/8.5 target the default `opcache.jit = disable` renders the
/// all-zero/false array, byte-identical to what this function returned before the mapping
/// existed. On an 8.2/8.3 target the default is `tracing`, so the default array now carries
/// `kind = 5, opt_level = 4, opt_flags = 6` (with the clamp still forcing the other four) instead
/// of the previous all-zero tuning fields — a correction, pinned to the 8.2.31 observation above.
///
/// `opcache.jit_buffer_size` is deliberately NOT read here: under the clamp both buffer figures
/// are 0 regardless of the directive, and reference PHP agrees (the 64M run above still reports
/// 0). The directive remains reported verbatim by `opcache_get_configuration()`/`ini_get`.
pub(super) fn render_jit_status(version_id: u32, overrides: &[(String, String)]) -> JitStatus {
    let config = effective_jit_config(version_id, overrides);
    JitStatus {
        // The clamp: no runtime JIT engine exists in an AOT binary.
        enabled: false,
        on: false,
        // The full reference mapping: what was CONFIGURED is reported verbatim.
        kind: config.kind,
        opt_level: config.opt_level,
        opt_flags: config.opt_flags,
        // The clamp: no JIT buffer is ever allocated.
        buffer_size: 0,
        buffer_free: 0,
    }
}
