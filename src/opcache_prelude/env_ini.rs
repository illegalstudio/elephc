//! Purpose:
//! Renders runtime environment overrides and OPcache INI helpers.
//!
//! Called from:
//! - The OPcache prelude facade and sibling rendering modules.
//!
//! Key details:
//! - Only reporting directives accept runtime overrides; compiled behavior stays frozen.

#[allow(unused_imports)]
use super::*;

/// Directive `name`'s effective TYPED value at runtime: the compile-time literal for a directive
/// outside the runtime-override scope ([`directive_runtime_overridable`]), otherwise a call into
/// the typed environment helper with the two environment-variable spellings and the compile-time
/// value as the default.
///
/// `value` is the EFFECTIVE compile-time value (defaults with `--ini` already applied), which is
/// what makes the precedence chain baked default → `--ini` → env fall out for free.
pub(super) fn directive_runtime_value_expr(name: &str, value: &DirectiveValue) -> Expr {
    let literal = directive_value_expr(value);
    if !directive_runtime_overridable(name) {
        return literal;
    }
    let (under, dotted) = directive_env_var_names(name);
    let helper = match directive_env_type_code(name, value) {
        'b' => "__elephc_opcache_env_bool",
        'i' => "__elephc_opcache_env_int",
        'p' => "__elephc_opcache_env_pct",
        'f' => "__elephc_opcache_env_float",
        // `opcache.jit_prof_threshold` in the 8.2 profile ONLY: a `zend_strtod` READ whose value is
        // REPORTED truncated to an int (php-src 8.2 uses `add_assoc_long` on a `double` field).
        // See `crate::opcache::directives::JIT_PROF_THRESHOLD`.
        't' => "__elephc_opcache_env_trunc",
        _ => "__elephc_opcache_env_str",
    };
    e_call(helper, vec![e_str(&under), e_str(&dotted), literal])
}

/// The shared `opcache.*` INI helper declarations for the compile target, baked from the
/// version-keyed directive table so CLI and `--web` share one source of truth. The raw strings and
/// access levels come from `directive_ini_string` / `directive_access` (byte-verified against
/// reference PHP 8.5.6), so this is a pure projection of the same table that backs
/// `opcache_get_configuration()`.
///
/// KEY ORDER: `ini_get_all` reports its keys SORTED ASCENDING, so the key list is a sorted COPY.
/// `opcache_directives()` itself keeps REGISTRATION order, which is what
/// `opcache_get_configuration()['directives']` reports and is byte-correct there — it must not be
/// reordered. Only this projection sorts.
pub(crate) fn ini_helper_declarations(
    php_version: PhpVersion,
    overrides: &[(String, String)],
) -> Program {
    let version_id = php_version.version_id();
    let directives = opcache_directives(version_id);

    // The raw INI string per opcache key. It is the user's `--ini` override verbatim when validly
    // overridden, else the default projection. A RUNTIME env override (`ELEPHC_INI_*`) applies to
    // the reporting-only directives: the arm yields the environment value VERBATIM when it parses
    // for the directive's type and the compile-time raw string otherwise. Excluded directives keep
    // the plain literal — honoring them here would make the binary contradict its own
    // `opcache_get_status()`.
    let string_arms = directives
        .iter()
        .map(|(name, value)| {
            let raw = effective_directive_ini_string(name, value, overrides);
            let arm = if directive_runtime_overridable(name) {
                // The type code is read off the DEFAULT value: `parse_ini_override` preserves the
                // `DirectiveValue` variant, so a `--ini` override never changes a directive's type.
                let (under, dotted) = directive_env_var_names(name);
                e_call(
                    "__elephc_opcache_env_raw",
                    vec![
                        e_str(&under),
                        e_str(&dotted),
                        e_str(&directive_env_type_code(name, value).to_string()),
                        e_str(&raw),
                    ],
                )
            } else {
                e_str(&raw)
            };
            ((*name).to_string(), arm)
        })
        .collect();

    // Whether `ini_get_all()` reports a directive's global_value / local_value as PHP `null`
    // rather than a string. Reference PHP does that for exactly the directives php-src registers
    // with a C NULL default AND that were never assigned a value — `opcache.file_cache` is the
    // only one in the block. A compile-time `--ini` ASSIGNS it, so the arm collapses to `false`;
    // otherwise it consults the RUNTIME environment override with the same "empty means unset"
    // rule the rest of the `ELEPHC_INI_*` surface uses.
    let null_arms = directives
        .iter()
        .filter(|(name, _)| directive_ini_null_default(name))
        .map(|(name, _)| {
            let condition = if latest_ini_override(overrides, name).is_some() {
                e_bool(false)
            } else {
                let (under, dotted) = directive_env_var_names(name);
                e_binop(
                    e_call(
                        "__elephc_opcache_env",
                        vec![e_str(&under), e_str(&dotted)],
                    ),
                    BinOp::StrictEq,
                    e_str(""),
                )
            };
            ((*name).to_string(), condition)
        })
        .collect();

    // 7 for the PHP_INI_ALL directives, 4 for the rest, and -1 for a non-opcache key.
    let all_names: Vec<&str> = directives
        .iter()
        .filter(|(name, _)| directive_access(name) == 7)
        .map(|(name, _)| *name)
        .collect();

    let mut keys: Vec<&str> = directives.iter().map(|(name, _)| *name).collect();
    keys.sort_unstable();

    build::ini_helper_decls(string_arms, null_arms, &all_names, &keys)
}

/// The RUNTIME `ELEPHC_INI_*` environment-override helper declarations.
///
/// WHY THIS EXISTS AT ALL. Every `opcache.*` directive is compiled into the binary, so the
/// natural analogue of `php -d` is elephc's compile-time `--ini KEY=VALUE`. That leaves no way to
/// re-point a directive on an ALREADY-BUILT binary, which is exactly what a deployment needs
/// (`ELEPHC_INI_opcache__save_comments=0 ./app`). Reference PHP has no per-directive environment
/// override to copy — VERIFIED on 8.5.6 — so this is a documented elephc EXTENSION, not a parity
/// feature. Precedence: baked default → `--ini` (compile time) → `ELEPHC_INI_*` (runtime, wins).
///
/// WHY IT IS PHP RATHER THAN RUST. A plain CLI binary links NO Rust staticlib — every elephc
/// runtime is an opt-in bridge selected in `crate::linker` — so a Rust-side override table would
/// force every binary to link one (killing pay-for-use) or need a hand-written `__rt_*` helper in
/// assembly for four targets. `getenv` is already a first-class codegen builtin with a CONCRETE
/// `Str` EIR result type, available identically on CLI and `--web`.
///
/// INJECTED EXACTLY ONCE. These are plain functions, so a second copy is a redeclaration error.
/// Under `--web` the web prelude emits them (see `crate::web_prelude`) and [`inject_if_used`]
/// does not; on CLI it is the other way round.
pub(crate) fn env_override_declarations() -> Program {
    build::env_override_helper_decls()
}
