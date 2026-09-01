//! Purpose:
//! The single source of truth for Zend OPcache's `opcache.*` INI directives and
//! their typed, normalized default values, keyed on the targeted PHP language
//! version. Backs the `opcache_get_configuration()` surface on both the native
//! AOT compiler (rendered to a PHP array literal by `crate::opcache_prelude`) and
//! the magician eval interpreter (rendered to runtime cells), so the two never
//! drift.
//!
//! Called from:
//! - `crate::opcache_prelude` (native, renders a PHP array-literal string).
//! - `crates/elephc-magician` builtins (eval, via a `#[path]` include — builds the
//!   equivalent runtime array), mirroring how `list_id_prelude::table` is shared.
//!
//! Key details:
//! - This module is intentionally dependency-free (no `crate::` references): it is
//!   compiled into both the `elephc` and `elephc-magician` crates through a shared
//!   file include, so it must not name types from either crate. The caller passes a
//!   plain `PHP_VERSION_ID` (80200/80300/80400/80510 for the maintained defaults).
//! - It owns the `--ini` OVERRIDE PIPELINE, and the ORDER of its two stages is load-bearing:
//!   [`ini_scanner_value`] first (php-src's INI *scanner* rewrites the boolean barewords
//!   `on`/`true`/`yes` → `"1"` and `off`/`false`/`no`/`none`/`null` → `""` for EVERY directive,
//!   ahead of every handler), then [`parse_ini_override`]'s type dispatch on the RESULT. Two of
//!   those handlers CANNOT FAIL, mirroring php-src: `parse_ini_bool` (`zend_ini_parse_bool` —
//!   `garbage` is `false`, not a rejection) and `parse_ini_quantity` (`zend_ini_parse_quantity` —
//!   `12abc` is `12` plus a warning). The handlers that CAN refuse a value are
//!   `opcache.max_wasted_percentage` (out of range), `opcache.memory_consumption` (below its
//!   8 MiB floor) and `opcache.jit` (invalid spelling); those three are the only ways an
//!   override leaves the compiled default in place. [`ini_override_warnings`] carries the
//!   quantity diagnostics out to the compiler's stderr, where reference PHP prints them at
//!   startup.
//! - It also owns the RUNTIME OVERRIDE METADATA the native compiler bakes into the prelude:
//!   [`RUNTIME_INI_ENV_PREFIX`] / [`directive_env_var_names`] (the two `ELEPHC_INI_*` spellings),
//!   [`directive_runtime_overridable`] (the SCOPE RULE — which directives may be re-pointed by an
//!   environment variable at run time and which are compile-time-only because elephc derives
//!   baked behavior from them), and [`directive_env_type_code`] (the type code the baked PHP
//!   normalizer switches on). They live here so the scope rule and the type dispatch sit next to
//!   the table they describe and cannot drift from [`parse_ini_override`].
//! - It also owns the `opcache.jit` MODE PARSER ([`apply_jit_setting`], [`parse_jit_mode`],
//!   [`effective_jit_config`]): the spelling → `kind`/`opt_level`/`opt_flags` mapping that
//!   backs `opcache_get_status()['jit']`. It lives here rather than in the prelude because it
//!   is a property of the directive, shared by every consumer, and because the invalid-spelling
//!   rule it encodes is the same one [`parse_ini_override`] needs to decide whether an
//!   `--ini opcache.jit=…` override is stored at all.
//! - Values are the compiled-in defaults *as `opcache_get_configuration()` reports
//!   them*: booleans as `true`/`false`, byte-size directives as integer byte counts
//!   (`opcache.memory_consumption` → 128 MiB = 134217728), `opcache.max_wasted_percentage`
//!   as the fraction `0.05` (not the raw `5`), and `opcache.optimization_level` as the
//!   decimal form of `0x7FFEBFFF` (2147401727). The 8.5 set is byte-verified against
//!   reference PHP 8.5 php-src oracle; the
//!   8.2/8.3/8.4 sets apply the documented per-version deltas to the same normalized
//!   values (their relative ordering is derived, not byte-verified against a live
//!   older runtime).

/// One typed, normalized directive default value as `opcache_get_configuration()`
/// reports it. The variants cover exactly the reported PHP scalar shapes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DirectiveValue {
    /// A PHP boolean (`OnUpdateBool` directive), reported as `true`/`false`.
    Bool(bool),
    /// A PHP integer (counts, byte sizes, bitmasks), reported as an `int`.
    Int(i64),
    /// A PHP float (`opcache.max_wasted_percentage`, `opcache.jit_prof_threshold`).
    Float(f64),
    /// A PHP string directive (paths, `opcache.jit` mode, empty defaults).
    Str(&'static str),
}

/// The OPcache product name reported under `['version']['opcache_product_name']`,
/// identical across 8.2–8.5 (`ACCELERATOR_PRODUCT_NAME`).
pub const OPCACHE_PRODUCT_NAME: &str = "Zend OPcache";

/// Returns the targeted PHP language-version string reported under
/// `['version']['version']`. The default profile is pinned to the frozen
/// `8.5.10-dev` php-src oracle; older profiles retain their `8.<minor>.0` form.
pub fn opcache_version_string(version_id: u32) -> &'static str {
    match version_id {
        80200 => "8.2.0",
        80300 => "8.3.0",
        80400 => "8.4.0",
        // 80500 and any newer/unknown id fall back to the newest maintained profile.
        _ => "8.5.10-dev",
    }
}

/// Returns the ordered list of `opcache.*` directives and their typed, normalized
/// default values for the targeted `PHP_VERSION_ID`. Order mirrors reference PHP's
/// reported order for the 8.5 case (byte-verified); older versions apply the
/// documented deltas: `opcache.consistency_checks` exists only in 8.2;
/// `opcache.jit`/`opcache.jit_buffer_size` differ before 8.4; `opcache.jit_hot_loop`
/// differs before 8.5; `opcache.jit_max_trace_length` is absent before 8.3;
/// `opcache.file_cache_read_only` is present only from 8.5; and
/// `opcache.jit_prof_threshold` is reported as an INT before 8.3 (see [`JIT_PROF_THRESHOLD`]).
pub fn opcache_directives(version_id: u32) -> Vec<(&'static str, DirectiveValue)> {
    use DirectiveValue::{Bool, Float, Int, Str};

    // Per-version default flips (see the OPCACHE_DIRECTIVE_MATRIX delta summary).
    let jit_mode = if version_id < 80400 { "tracing" } else { "disable" };
    let jit_buffer_size: i64 = if version_id < 80400 { 0 } else { 67_108_864 };
    let jit_hot_loop: i64 = if version_id < 80500 { 64 } else { 61 };
    // `opcache.jit_prof_threshold` is a C `double` in every maintained version, but 8.2's
    // `opcache_get_configuration()` reports it with `add_assoc_long` (an implicit double→long
    // TRUNCATION), while 8.3+ report it with `add_assoc_double`. See JIT_PROF_THRESHOLD.
    let jit_prof_threshold = if version_id < 80300 {
        Int(0)
    } else {
        Float(0.005)
    };

    let mut directives: Vec<(&'static str, DirectiveValue)> = Vec::with_capacity(54);
    directives.push(("opcache.enable", Bool(true)));
    directives.push(("opcache.enable_cli", Bool(false)));
    directives.push(("opcache.use_cwd", Bool(true)));
    directives.push(("opcache.validate_timestamps", Bool(true)));
    directives.push(("opcache.validate_permission", Bool(false)));
    directives.push(("opcache.validate_root", Bool(false)));
    directives.push(("opcache.dups_fix", Bool(false)));
    directives.push(("opcache.revalidate_path", Bool(false)));
    directives.push(("opcache.log_verbosity_level", Int(1)));
    directives.push(("opcache.memory_consumption", Int(134_217_728)));
    directives.push(("opcache.interned_strings_buffer", Int(8)));
    directives.push(("opcache.max_accelerated_files", Int(10_000)));
    directives.push(("opcache.max_wasted_percentage", Float(0.05)));
    if version_id < 80300 {
        // 8.2-only directive, registered immediately after `max_wasted_percentage`.
        // Reported as an integer request-count (`OnUpdateConsistencyChecks`); default 0.
        directives.push(("opcache.consistency_checks", Int(0)));
    }
    directives.push(("opcache.force_restart_timeout", Int(180)));
    directives.push(("opcache.revalidate_freq", Int(2)));
    directives.push(("opcache.preferred_memory_model", Str("")));
    directives.push(("opcache.blacklist_filename", Str("")));
    directives.push(("opcache.max_file_size", Int(0)));
    directives.push(("opcache.error_log", Str("")));
    directives.push(("opcache.protect_memory", Bool(false)));
    directives.push(("opcache.save_comments", Bool(true)));
    directives.push(("opcache.record_warnings", Bool(false)));
    directives.push(("opcache.enable_file_override", Bool(false)));
    directives.push(("opcache.optimization_level", Int(2_147_401_727)));
    directives.push(("opcache.lockfile_path", Str("/tmp")));
    directives.push(("opcache.file_cache", Str("")));
    if version_id >= 80500 {
        // Added in 8.5, placed immediately before `opcache.file_cache_only`.
        directives.push(("opcache.file_cache_read_only", Bool(false)));
    }
    directives.push(("opcache.file_cache_only", Bool(false)));
    directives.push(("opcache.file_cache_consistency_checks", Bool(true)));
    directives.push(("opcache.file_update_protection", Int(2)));
    directives.push(("opcache.opt_debug_level", Int(0)));
    directives.push(("opcache.restrict_api", Str("")));
    directives.push(("opcache.huge_code_pages", Bool(false)));
    directives.push(("opcache.preload", Str("")));
    directives.push(("opcache.preload_user", Str("")));
    directives.push(("opcache.jit", Str(jit_mode)));
    directives.push(("opcache.jit_buffer_size", Int(jit_buffer_size)));
    directives.push(("opcache.jit_debug", Int(0)));
    directives.push(("opcache.jit_bisect_limit", Int(0)));
    directives.push(("opcache.jit_blacklist_root_trace", Int(16)));
    directives.push(("opcache.jit_blacklist_side_trace", Int(8)));
    directives.push(("opcache.jit_hot_func", Int(127)));
    directives.push(("opcache.jit_hot_loop", Int(jit_hot_loop)));
    directives.push(("opcache.jit_hot_return", Int(8)));
    directives.push(("opcache.jit_hot_side_exit", Int(8)));
    directives.push(("opcache.jit_max_exit_counters", Int(8192)));
    directives.push(("opcache.jit_max_loop_unrolls", Int(8)));
    directives.push(("opcache.jit_max_polymorphic_calls", Int(2)));
    directives.push(("opcache.jit_max_recursive_calls", Int(2)));
    directives.push(("opcache.jit_max_recursive_returns", Int(2)));
    directives.push(("opcache.jit_max_root_traces", Int(1024)));
    directives.push(("opcache.jit_max_side_traces", Int(128)));
    directives.push(("opcache.jit_prof_threshold", jit_prof_threshold));
    if version_id >= 80300 {
        // Added in 8.3; reported last, matching reference PHP's registration order.
        directives.push(("opcache.jit_max_trace_length", Int(1024)));
    }
    directives
}

/// Returns the RAW INI STRING for the directive `name` carrying value `value` — the
/// exact string `ini_get('opcache.<dir>')` reports in reference PHP, which is NOT the
/// same as the normalized `opcache_get_configuration()` value.
///
/// The projection is: booleans → `"1"`/`"0"`; integers → their decimal; floats → their
/// shortest round-tripping decimal; strings → themselves (empty → `""`). Four directives
/// carry a raw INI string that CANNOT be derived from the normalized value and so are
/// overridden explicitly (byte-verified against reference PHP 8.5.6 `ini_get`):
/// - `opcache.memory_consumption` → `"128"` (raw MiB, not the byte count `134217728`),
/// - `opcache.max_wasted_percentage` → `"5"` (raw percent, not the fraction `0.05`),
/// - `opcache.optimization_level` → `"0x7FFEBFFF"` (raw hex, not the decimal `2147401727`),
/// - `opcache.jit_buffer_size` → `"64M"` for 8.4/8.5 (raw size string, not `67108864`),
///   and `"0"` for 8.2/8.3 (where the normalized default is `Int(0)`, which the fall-
///   through would render `"0"` anyway — the override is keyed on the normalized int so
///   the two stay consistent).
///
/// This is NEW data: it is the source of truth for the `ini_get('opcache.*')` surface on
/// both the native AOT compiler (`crate::opcache_prelude`) and the eval interpreter, exactly
/// as `opcache_directives` backs `opcache_get_configuration()`.
/// `allow(dead_code)`: this file is `#[path]`-included into `elephc-magician` too, which does
/// not (yet) consult the raw-string projection, so it is dead there while live in `elephc`.
#[allow(dead_code)]
pub fn directive_ini_string(name: &str, value: &DirectiveValue) -> String {
    // Overrides for directives whose raw INI string is not derivable from the normalized
    // `opcache_get_configuration()` value.
    match name {
        "opcache.memory_consumption" => return "128".to_string(),
        "opcache.max_wasted_percentage" => return "5".to_string(),
        "opcache.optimization_level" => return "0x7FFEBFFF".to_string(),
        // The registered INI default string is `"0.005"` on EVERY maintained version; only the
        // NORMALIZED projection differs (8.2 truncates it to `0` — see `JIT_PROF_THRESHOLD`), so
        // the raw string must not be derived from the normalized value there.
        // VERIFIED on real PHP 8.2.31: `ini_get('opcache.jit_prof_threshold')` is `'0.005'` while
        // `opcache_get_configuration()` reports `int(0)`.
        JIT_PROF_THRESHOLD => return "0.005".to_string(),
        "opcache.jit_buffer_size" => {
            // 8.2/8.3 default is the raw `0`; 8.4/8.5 default is the raw size string `64M`.
            // Keyed on the normalized int so the two representations cannot drift apart.
            return match value {
                DirectiveValue::Int(0) => "0".to_string(),
                _ => "64M".to_string(),
            };
        }
        _ => {}
    }
    match value {
        // Reference PHP renders opcache booleans as `"1"`/`"0"` (unlike session's `"1"`/`""`).
        DirectiveValue::Bool(true) => "1".to_string(),
        DirectiveValue::Bool(false) => "0".to_string(),
        DirectiveValue::Int(int) => int.to_string(),
        // Shortest round-tripping decimal (`0.005` for `opcache.jit_prof_threshold`).
        DirectiveValue::Float(float) => float.to_string(),
        DirectiveValue::Str(string) => string.to_string(),
    }
}

/// Returns the most recent raw INI override string supplied for `name`, or `None` when the
/// directive was never overridden. Repeated `--ini` flags for the same key are last-wins
/// (matching how a later `-d` on a PHP command line overrides an earlier one), so the scan
/// runs from the back.
/// `allow(dead_code)`: dead in the `elephc-magician` `#[path]` includes that do not consult
/// the override machinery (eval has no `--ini`), live in `elephc`.
#[allow(dead_code)]
fn latest_override<'a>(overrides: &'a [(String, String)], name: &str) -> Option<&'a str> {
    overrides
        .iter()
        .rev()
        .find(|(key, _)| key.as_str() == name)
        .map(|(_, value)| value.as_str())
}

/// Applies PHP's INI SCANNER boolean-alias rewrite to a raw override string, returning the value
/// the directive's TYPE HANDLER actually sees.
///
/// php-src's `zend_ini_scanner.l` rewrites a fixed set of bareword spellings in the VALUE
/// position, case-insensitively, BEFORE any directive handler runs — and it does so for EVERY
/// directive, not just the boolean ones:
///
/// ```text
/// <ST_VALUE>("true"|"on"|"yes"){TABS_AND_SPACES}*            → BOOL_TRUE,  value "1"
/// <ST_VALUE>("false"|"off"|"no"|"none"|"null"){TABS_AND_SPACES}* → BOOL_FALSE, value ""
/// ```
///
/// So the rewrite is a property of the SCANNER, and the string `ini_get()` echoes is the
/// REWRITTEN one, not the spelling the user typed. VERIFIED on reference PHP 8.5.6 against a
/// plain STRING directive (no bool handler anywhere on the path):
///
/// | `-d opcache.preferred_memory_model=` | `ini_get()` |
/// |--------------------------------------|-------------|
/// | `on` / `On` / `ON` / `oN`            | `'1'`       |
/// | `true` / `True` / `TRUE`             | `'1'`       |
/// | `yes` / `Yes` / `YES`                | `'1'`       |
/// | `off` / `Off` / `OFF`                | `''`        |
/// | `false` / `False` / `FALSE`          | `''`        |
/// | `no` / `No` / `NO`                   | `''`        |
/// | `none` / `None` / `NONE` / `nOnE`    | `''`        |
/// | `null` / `NULL`                      | `''`        |
/// | `1`                                  | `'1'`       |
/// | `0`                                  | `'0'`       |
///
/// Matching is case-insensitive and the surrounding whitespace is part of the token
/// (`opcache.preferred_memory_model =  on ` in a php.ini file reports `'1'`; VERIFIED with a
/// real `-c` ini file), which is why the comparison runs on the TRIMMED value. A value that
/// matches no alias is returned VERBATIM — including its surrounding whitespace — because a
/// QUOTED INI value bypasses the rewrite entirely (`opcache.error_log = "  on  "` reports
/// `'  on  '`; VERIFIED), and elephc's `--ini KEY=VALUE` takes its value literally.
///
/// ORDER MATTERS: this runs FIRST and the directive's type handler runs on the RESULT. That is
/// what makes `--ini opcache.jit=on` report `ini_get() === '1'` while still selecting the
/// TRACING jit (`"1"` is one of `apply_jit_setting`'s tracing spellings), and what makes
/// `--ini opcache.max_wasted_percentage=off` fall back to the compiled default (`""` → `atoi`
/// `0` → out of range → the store is refused; VERIFIED `ini_get()` = `'5'`).
///
/// MIRRORED IN PHP by `__elephc_ini_scan` in `crate::opcache_prelude::build`'s environment
/// which the runtime `ELEPHC_INI_*` path applies at exactly the same point.
#[allow(dead_code)]
pub fn ini_scanner_value(raw: &str) -> &str {
    let trimmed = raw.trim();
    const TRUE_ALIASES: [&str; 3] = ["on", "true", "yes"];
    const FALSE_ALIASES: [&str; 5] = ["off", "false", "no", "none", "null"];
    if TRUE_ALIASES
        .iter()
        .any(|alias| trimmed.eq_ignore_ascii_case(alias))
    {
        return "1";
    }
    if FALSE_ALIASES
        .iter()
        .any(|alias| trimmed.eq_ignore_ascii_case(alias))
    {
        return "";
    }
    raw
}

/// Parses a SCANNER-REWRITTEN INI string as an OPcache boolean directive value.
///
/// php-src's `zend_ini_parse_bool` CANNOT FAIL — there is no "invalid boolean" outcome, only
/// `true` and `false`:
///
/// ```c
/// ZEND_API bool ZEND_FASTCALL zend_ini_parse_bool(zend_string *str) {
///     if (zend_string_equals_literal_ci(str, "true")
///      || zend_string_equals_literal_ci(str, "yes")
///      || zend_string_equals_literal_ci(str, "on")) {
///         return true;
///     }
///     return (bool) atoi(ZSTR_VAL(str));
/// }
/// ```
///
/// so anything that is neither one of those three words nor a string whose `atoi` is non-zero
/// falls to `false`. VERIFIED on reference PHP 8.5.6 (`-d opcache.save_comments=<v>`, reading
/// `opcache_get_configuration()['directives']` and `ini_get()`):
///
/// | raw       | seen by the handler | `directives` | `ini_get` |
/// |-----------|---------------------|--------------|-----------|
/// | `garbage` | `garbage`           | `false`      | `'garbage'` |
/// | `2`       | `2`                 | `true`       | `'2'`     |
/// | `-1`      | `-1`                | `true`       | `'-1'`    |
/// | `on`      | `1` (scanner)       | `true`       | `'1'`     |
/// | `On`      | `1` (scanner)       | `true`       | `'1'`     |
/// | `TRUE`    | `1` (scanner)       | `true`       | `'1'`     |
/// | `yes`     | `1` (scanner)       | `true`       | `'1'`     |
/// | `none`    | `` (scanner)        | `false`      | `''`      |
///
/// The `2` and `-1` rows are the `atoi` tail: they are truthy WITHOUT being a recognized
/// spelling. An earlier revision of this function returned `Option<bool>` and answered `None`
/// for every one of `garbage`, `2` and `-1`, which made elephc keep the compiled default where
/// reference PHP stores `false`, `true` and `true` respectively.
///
/// NOT TRIMMED, deliberately: `zend_ini_parse_bool` compares whole `zend_string`s, so a padded
/// ` on ` is NOT the word `on` there either. It never reaches this function with padding anyway
/// — [`ini_scanner_value`] has already rewritten the padded alias to `"1"`/`""` — and the
/// residual cases agree with `atoi`'s own leading-whitespace skip.
#[allow(dead_code)]
fn parse_ini_bool(raw: &str) -> bool {
    if raw.eq_ignore_ascii_case("true")
        || raw.eq_ignore_ascii_case("yes")
        || raw.eq_ignore_ascii_case("on")
    {
        return true;
    }
    parse_ini_atoi(raw) != 0
}

/// The `opcache.memory_consumption` directive name — the one integer directive php-src does NOT
/// register with the generic `OnUpdateLong`/quantity handler (see [`parse_ini_int`]).
const MEMORY_CONSUMPTION: &str = "opcache.memory_consumption";

/// The `opcache.max_accelerated_files` directive name — an `atoi`-read, RANGE-VALIDATED integer
/// (see [`directive_int_range`]).
const MAX_ACCELERATED_FILES: &str = "opcache.max_accelerated_files";

/// The `opcache.jit_prof_threshold` directive name.
///
/// It is a C `double` (`OnUpdateReal`) in every maintained version, so its INI parse is ALWAYS the
/// `zend_strtod` leading-prefix read ([`parse_ini_float_prefix`]) — never the quantity parser —
/// even in the 8.2 profile where the table stores it as a [`DirectiveValue::Int`].
///
/// WHY THE TABLE TYPE VARIES. `opcache_get_configuration()` reports it with a DIFFERENT php-src
/// call per version, and the C implicit conversion in the 8.2 form truncates toward zero:
///
/// | version | `ext/opcache/zend_accelerator_module.c`                        | reported |
/// |---------|----------------------------------------------------------------|----------|
/// | 8.2     | `add_assoc_long(&directives, …, JIT_G(prof_threshold))`         | `int`    |
/// | 8.3+    | `add_assoc_double(&directives, …, JIT_G(prof_threshold))`       | `float`  |
///
/// VERIFIED two ways. (1) Real PHP 8.2.31 (`/opt/homebrew/opt/php@8.2/bin/php -d opcache.enable=1
/// -d opcache.enable_cli=1`) reports `int(0)` for the default and, with
/// `-d opcache.jit_prof_threshold=<v>`, `2.7 → int(2)`, `0.5 → int(0)`, `-1.9 → int(-1)` — the
/// TRUNCATION of the double, not a quantity read (a quantity read of `-1.9` also gives `-1`, but
/// `0x10` would give `16` where the double read gives `0`). PHP 8.5.6 reports `float(0.005)`.
/// (2) The php-src `PHP-8.2` / `PHP-8.3` / `PHP-8.4` / `PHP-8.5` branches carry exactly the two
/// call forms above, which is what settles 8.3 — no real 8.3 build exists on this host
/// (`php@8.3` is a symlink to 8.5), so 8.3 is DERIVED FROM SOURCE, not probed: it uses
/// `add_assoc_double` and therefore reports a FLOAT, like 8.4/8.5 and unlike 8.2.
const JIT_PROF_THRESHOLD: &str = "opcache.jit_prof_threshold";

/// Returns the CLOSED range `[lo, hi]` of values php-src's handler for integer directive `name`
/// ACCEPTS, or `None` when the handler accepts everything the parser produces.
///
/// A value outside the range is REFUSED (`return FAILURE`), which leaves the compiled-in default
/// in place for BOTH `opcache_get_configuration()` and `ini_get()` — php-src never stores a
/// clamped value. [`parse_ini_int`] enforces this by returning `None`, the caller's "keep the
/// default" signal.
///
/// THE ACCEPTED RANGE IS NOT ALWAYS THE ONE THE WARNING NAMES. Five of these handlers test with a
/// STRICT `<` against a `MAX` constant while their message prints that same constant as an
/// inclusive upper bound, so the message overstates the range by one. Both facts are reproduced:
/// the range here, the message text in [`ini_range_warning`].
///
/// | directive                            | php-src test                | accepted | message says |
/// |--------------------------------------|-----------------------------|----------|--------------|
/// | `max_accelerated_files`              | `>= 200 && <= 1000000`      | 200…1000000 | (silent)  |
/// | `interned_strings_buffer`            | `>= 0 && <= 32767`          | 0…32767  | (silent)     |
/// | `jit_blacklist_root_trace`           | `>= 0 && < 256`             | 0…255    | 0 and 255    |
/// | `jit_blacklist_side_trace`           | `>= 0 && < 256`             | 0…255    | 0 and 255    |
/// | `jit_hot_func`                       | `>= 0 && < 256`             | 0…255    | 0 and 255    |
/// | `jit_hot_loop`                       | `>= 0 && < 256`             | 0…255    | 0 and 255    |
/// | `jit_hot_return`                     | `>= 0 && < 256`             | 0…255    | 0 and 255    |
/// | `jit_hot_side_exit`                  | `>= 0 && < 256`             | 0…255    | 0 and 255    |
/// | `jit_max_loop_unrolls`               | `> 0 && < 10`               | 1…9      | 1 and 10     |
/// | `jit_max_recursive_calls`            | `> 0 && < 10`               | 1…9      | 1 and 10     |
/// | `jit_max_recursive_returns`          | `>= 0 && < 4`               | 0…3      | 0 and 4      |
/// | `jit_max_trace_length`               | `> 3 && <= 1024`            | 4…1024   | 4 and 1024   |
///
/// VERIFIED on reference PHP 8.5.6 (`php -d xdebug.mode=off -d opcache.enable=1
/// -d opcache.enable_cli=1 -d opcache.jit=tracing -d opcache.<name>=<v>`, reading
/// `opcache_get_configuration()['directives']`), boundary by boundary:
/// `max_accelerated_files` 199 → 10000, 200 → 200, 1000000 → 1000000, 1000001 → 10000;
/// `interned_strings_buffer` -1 → 8, 0 → 0, 32767 → 32767 (accepted; it then fatals on the SHM
/// allocation, which is how the acceptance is observable), 32768 → 8, 100000 → 8;
/// `jit_max_loop_unrolls` 0 → 8, 9 → 9, **10 → 8**, 11 → 8; `jit_max_recursive_calls` 9 → 9,
/// **10 → 2**; `jit_max_recursive_returns` 3 → 3, **4 → 2**, 5 → 2; `jit_max_trace_length` 3 →
/// 1024, 4 → 4, 1024 → 1024, 1025 → 1024; `jit_hot_func` -1 → 127, 0 → 0, 255 → 255, 256 → 127.
/// The three bolded rows are the off-by-one quirk: they are the values the WARNING calls legal.
///
/// The 32767 ceiling is `MAX_INTERNED_STRINGS_BUFFER_SIZE`, a compile-time `MIN` of three
/// overflow guards in `zend_accelerator_module.c` that evaluates to
/// `UINT32_MAX / (32 * 1024 * sizeof(uint32_t))` = 32767 on every 64-bit build; the reference
/// build's own diagnostic prints it verbatim (`must be less than or equal to 32767`, observed
/// with `-d opcache.log_verbosity_level=2`).
#[allow(dead_code)]
pub fn directive_int_range(name: &str) -> Option<(i64, i64)> {
    Some(match name {
        MAX_ACCELERATED_FILES => (200, 1_000_000),
        "opcache.interned_strings_buffer" => (0, 32_767),
        "opcache.jit_blacklist_root_trace"
        | "opcache.jit_blacklist_side_trace"
        | "opcache.jit_hot_func"
        | "opcache.jit_hot_loop"
        | "opcache.jit_hot_return"
        | "opcache.jit_hot_side_exit" => (0, 255),
        "opcache.jit_max_loop_unrolls" | "opcache.jit_max_recursive_calls" => (1, 9),
        "opcache.jit_max_recursive_returns" => (0, 3),
        "opcache.jit_max_trace_length" => (4, 1024),
        _ => return None,
    })
}

/// Returns the VERBATIM `E_WARNING` body reference PHP prints when integer directive `name` is
/// given an out-of-range value, or `None` when the refusal is silent at the default
/// `opcache.log_verbosity_level`.
///
/// TWO FAMILIES, TWO CHANNELS. The ten JIT tuning validators live in the JIT INI block and report
/// through `zend_error(E_WARNING, …)`, so they print unconditionally as a startup
/// `Warning: <body> in Unknown on line 0`. `opcache.max_accelerated_files` and
/// `opcache.interned_strings_buffer` instead report through `zend_accel_error(ACCEL_LOG_WARNING,
/// …)`, which is gated on `opcache.log_verbosity_level >= 2` and therefore prints NOTHING at the
/// default verbosity of 1 — the refusal is silent, and only the reverted value is observable.
/// Those two return `None` here; modelling their verbosity-gated log line would mean modelling
/// `zend_accel_error`'s whole timestamped `<date> (<pid>): Warning <body>` channel, which elephc
/// has no counterpart for (its `--ini` diagnostics go to the compiler's stderr).
///
/// AND TWO MESSAGE SHAPES within the JIT family, which is why this is a lookup and not a format:
/// the six `OnUpdateCounter` directives interpolate `; using default value instead.` where the
/// four unroll/length validators do not. VERIFIED byte-for-byte on reference PHP 8.5.6
/// (`php -d opcache.enable=1 -d opcache.enable_cli=1 -d opcache.jit=tracing
/// -d opcache.<name>=99999 x.php`, stderr):
///
/// ```text
/// Warning: Invalid "opcache.jit_hot_func" setting; using default value instead. Should be between 0 and 255 in Unknown on line 0
/// Warning: Invalid "opcache.jit_max_loop_unrolls" setting. Should be between 1 and 10 in Unknown on line 0
/// Warning: Invalid "opcache.jit_max_recursive_calls" setting. Should be between 1 and 10 in Unknown on line 0
/// Warning: Invalid "opcache.jit_max_recursive_returns" setting. Should be between 0 and 4 in Unknown on line 0
/// Warning: Invalid "opcache.jit_max_trace_length" setting. Should be between 4 and 1024 in Unknown on line 0
/// ```
///
/// The bodies below are those lines minus the `Warning: ` prefix and the ` in Unknown on line 0`
/// suffix — the same shape [`ini_override_warnings`] already returns for the quantity
/// diagnostics, which the compiler prints as `Warning: <body>`.
#[allow(dead_code)]
fn ini_range_warning(name: &str) -> Option<String> {
    let bound = match name {
        "opcache.jit_blacklist_root_trace"
        | "opcache.jit_blacklist_side_trace"
        | "opcache.jit_hot_func"
        | "opcache.jit_hot_loop"
        | "opcache.jit_hot_return"
        | "opcache.jit_hot_side_exit" => {
            return Some(format!(
                "Invalid \"{name}\" setting; using default value instead. \
                 Should be between 0 and 255"
            ))
        }
        // The four validators whose message omits `; using default value instead.` and whose
        // printed upper bound is the EXCLUSIVE constant (one past the last accepted value).
        "opcache.jit_max_loop_unrolls" | "opcache.jit_max_recursive_calls" => (1, 10),
        "opcache.jit_max_recursive_returns" => (0, 4),
        "opcache.jit_max_trace_length" => (4, 1024),
        _ => return None,
    };
    Some(format!(
        "Invalid \"{name}\" setting. Should be between {} and {}",
        bound.0, bound.1
    ))
}

/// The prime hash-table capacities `zend_accel_hash_init` rounds `opcache.max_accelerated_files`
/// up to, in php-src's own order (`ext/opcache/zend_accel_hash.c`).
const ACCEL_HASH_PRIMES: [i64; 18] = [
    5, 11, 19, 53, 107, 223, 463, 983, 1979, 3907, 7963, 16229, 32531, 65407, 130987, 262237,
    524521, 1048793,
];

/// Returns `opcache_get_status()['opcache_statistics']['max_cached_keys']` for a cache sized with
/// `max_accelerated_files`: the FIRST entry of [`ACCEL_HASH_PRIMES`] that is `>= ` it.
///
/// php-src (`zend_accel_hash_init`) walks the table with `if (hash_size <= prime_numbers[i])` and
/// takes the first hit, leaving `hash_size` untouched if it exceeds every prime — so the rule is
/// "first prime GREATER THAN OR EQUAL TO", not "strictly greater". The difference is observable at
/// each table value itself.
///
/// VERIFIED on reference PHP 8.5.6 (`php -d opcache.enable=1 -d opcache.enable_cli=1
/// -d opcache.max_accelerated_files=<n>`, reading
/// `opcache_get_status(false)['opcache_statistics']['max_cached_keys']`):
///
/// | `max_accelerated_files` | reported | note                              |
/// |-------------------------|----------|-----------------------------------|
/// | 200                     | 223      |                                   |
/// | 201                     | 223      |                                   |
/// | 222                     | 223      |                                   |
/// | **223**                 | **223**  | the `<=` case — NOT 463           |
/// | 224                     | 463      |                                   |
/// | 462 / 463               | 463      |                                   |
/// | 464                     | 983      |                                   |
/// | 1000                    | 1979     |                                   |
/// | 3000                    | 3907     |                                   |
/// | 10000 (default)         | 16229    |                                   |
/// | 65536                   | 130987   |                                   |
/// | 999999 / 1000000        | 1048793  | the directive maximum             |
///
/// The first five primes are unreachable through this directive (its floor is 200) but are kept so
/// the table is php-src's verbatim.
#[allow(dead_code)]
pub fn accel_hash_max_num_entries(max_accelerated_files: i64) -> i64 {
    for prime in ACCEL_HASH_PRIMES {
        if max_accelerated_files <= prime {
            return prime;
        }
    }
    max_accelerated_files
}

/// Whether `byte` is one of the six characters C's `isspace()` (and therefore php-src's
/// `ZEND_IS_SPACE`) treats as whitespace: space, `\t`, `\n`, `\v`, `\f`, `\r`.
fn is_ini_space(byte: u8) -> bool {
    byte == b' ' || (0x09..=0x0d).contains(&byte)
}

/// The digit value of `byte` in `radix`, or `None` when it is not a digit of that radix.
fn ini_digit_value(byte: u8, radix: u32) -> Option<u32> {
    let value = match byte {
        b'0'..=b'9' => u32::from(byte - b'0'),
        b'a'..=b'z' => u32::from(byte - b'a') + 10,
        b'A'..=b'Z' => u32::from(byte - b'A') + 10,
        _ => return None,
    };
    if value < radix {
        Some(value)
    } else {
        None
    }
}

/// Parses a SCANNER-REWRITTEN INI string as a QUANTITY (php-src's `zend_ini_parse_quantity`),
/// returning the stored value together with the diagnostic php-src would emit, if any.
///
/// LIKE [`parse_ini_bool`], THIS CANNOT FAIL. `zend_ini_parse_quantity` has no rejection path: a
/// malformed quantity produces a value plus an `errstr`, and the generic `OnUpdateLong` handler
/// stores that value and merely WARNS. So `ini_get()` echoes the raw string verbatim even for a
/// value reference PHP calls invalid. VERIFIED on reference PHP 8.5.6 (`-d opcache.max_file_size=<v>`):
///
/// | raw                    | stored           | warning                                            |
/// |------------------------|------------------|----------------------------------------------------|
/// | `12`                   | `12`             | —                                                  |
/// | `+12` / `-12`          | `12` / `-12`     | —                                                  |
/// | `12K` / `12k`          | `12288`          | —                                                  |
/// | `12M` / `12G`          | `12582912` / `12884901888` | —                                        |
/// | `12  M`                | `12582912`       | — (whitespace before the multiplier is legal)      |
/// | `0x10` / `0X10`        | `16`             | — (hex)                                            |
/// | `010` / `0777`         | `8` / `511`      | — (octal)                                          |
/// | `0b101` / `+0b11`      | `5` / `3`        | — (binary)                                         |
/// | `0x1G` / `0b1K`        | `1073741824` / `1024` | — (base prefix AND multiplier)                |
/// | `-0x10`                | `-16`            | —                                                  |
/// | `` / `  `              | `0`              | —                                                  |
/// | `garbage` / `K` / `x`  | `0`              | `no valid leading digits, interpreting as "0"`     |
/// | `-garbage` / `+garbage`| `0`              | `no valid leading digits, interpreting as "0"`     |
/// | `0xZZ` / `0b2`         | `0`              | `no valid leading digits, interpreting as "0"`     |
/// | `0x` / `0X` / `0b`     | `0`              | `no digits after base prefix, interpreting as "0"` |
/// | `12abc`                | `12`             | `unknown multiplier "c", interpreting as "12"`     |
/// | `12KB`                 | `12`             | `unknown multiplier "B", interpreting as "12"`     |
/// | `12.9` / `1e3` / `12,5`| `12` / `1` / `12`| `unknown multiplier "9"/"3"/"5"`                    |
/// | `08`                   | `0`              | `unknown multiplier "8", interpreting as "0"`      |
/// | `12 x` / `12 M x`      | `12`             | `unknown multiplier "x", interpreting as "12 "`    |
/// | `1  2`                 | `1`              | `unknown multiplier "2", interpreting as "1  "`    |
/// | `12MM` / `12M M`       | `12582912`       | `interpreting as "12M"` (trailing-data form)       |
/// | `9M M M`               | `9437184`        | `interpreting as "9M"`                             |
/// | `12 x M`               | `12582912`       | `interpreting as "12 M"`                           |
/// | `12 K junk`            | `12288`          | `interpreting as "12 k"`                           |
/// | `18446744073709551615` | `-1`             | `value is out of range, using overflow result`     |
/// | `9223372036854775808`  | `i64::MIN`       | `value is out of range, using overflow result`     |
/// | `-99999999999999999999`| `-1`             | `value is out of range, using overflow result`     |
///
/// THE THREE NON-OBVIOUS RULES, each of which the table above pins:
///
/// 1. THE MULTIPLIER IS THE LAST CHARACTER, not the first character after the digits. That is why
///    `12abc` reports `unknown multiplier "c"` (not `"a"`) and why `12 x M` is *multiplied* by
///    1 MiB despite the stray `x`.
/// 2. `interpreting as` IS A SLICE OF THE ORIGINAL STRING, not a rendering of the number. It runs
///    from the START of the value (leading whitespace and sign included) to the first non-space
///    after the digits — hence `"12 "` and `"1  "` with their trailing spaces. The trailing-data
///    form appends the MULTIPLIER CHARACTER AS WRITTEN, which is why `12 K junk` reports
///    `"12 k"` — lowercase, taken from `junk`'s final `k`, not from the `K` the user typed.
/// 3. THE ACCUMULATOR IS UNSIGNED. php-src reads the whole sign-carrying string with `strtoul`
///    and casts the `zend_ulong` result to `zend_long`, so `18446744073709551615` becomes `-1`
///    rather than saturating at `i64::MAX`, and an out-of-range magnitude yields `ULONG_MAX`
///    (→ `-1`) regardless of sign.
///
/// The returned diagnostic is the BODY only (`Invalid quantity …`); [`ini_override_warnings`]
/// wraps it in the `Invalid "<directive>" setting.` envelope php-src prints around it.
#[allow(dead_code)]
fn parse_ini_quantity(raw: &str) -> (i64, Option<String>) {
    let bytes = raw.as_bytes();
    // php-src returns 0 for an empty value before it looks at anything else.
    if bytes.is_empty() {
        return (0, None);
    }
    let mut start = 0;
    while start < bytes.len() && is_ini_space(bytes[start]) {
        start += 1;
    }
    let mut end = bytes.len();
    while end > start && is_ini_space(bytes[end - 1]) {
        end -= 1;
    }
    // All whitespace: `strtoul` consumes nothing and php-src reports 0 WITHOUT a diagnostic
    // (VERIFIED: `-d opcache.max_file_size='  '` stores 0 silently).
    if start == end {
        return (0, None);
    }

    let negative = bytes[start] == b'-';
    let mut cursor = start;
    if bytes[cursor] == b'+' || bytes[cursor] == b'-' {
        cursor += 1;
    }
    let no_digits = |message: &str| {
        (
            0_i64,
            Some(format!(
                "Invalid quantity \"{raw}\": {message}, interpreting as \"0\" for backwards compatibility"
            )),
        )
    };
    // php-src's `if (!ZEND_IS_DIGIT(*str)) goto invalid;` — checked on the character AFTER the
    // sign, which is why `-garbage` reports "no valid leading digits" rather than parsing `-0`.
    if cursor >= end || !bytes[cursor].is_ascii_digit() {
        return no_digits("no valid leading digits");
    }

    // Base detection. A leading `0` selects octal and is itself consumed as the first octal
    // digit, which is what makes `08` stop after the `0` (value 0) and hand `8` to the
    // multiplier check.
    let (radix, mut cursor) = if bytes[cursor] == b'0' && cursor + 1 < end {
        match bytes[cursor + 1] | 0x20 {
            b'x' => (16_u32, cursor + 2),
            b'b' => (2_u32, cursor + 2),
            _ => (8_u32, cursor),
        }
    } else if bytes[cursor] == b'0' {
        (8_u32, cursor)
    } else {
        (10_u32, cursor)
    };
    if cursor >= end {
        return no_digits("no digits after base prefix");
    }
    if ini_digit_value(bytes[cursor], radix).is_none() {
        return no_digits("no valid leading digits");
    }

    let mut magnitude: u128 = 0;
    let mut overflowed = false;
    while cursor < end {
        let Some(digit) = ini_digit_value(bytes[cursor], radix) else {
            break;
        };
        if !overflowed {
            magnitude = magnitude * u128::from(radix) + u128::from(digit);
            if magnitude > u128::from(u64::MAX) {
                overflowed = true;
            }
        }
        cursor += 1;
    }
    // `strtoul` returns ULONG_MAX on ERANGE whatever the sign; otherwise the two's-complement
    // negation of the magnitude, cast straight to `zend_long`.
    let unsigned = if overflowed {
        u64::MAX
    } else if negative {
        (magnitude as u64).wrapping_neg()
    } else {
        magnitude as u64
    };
    let value = unsigned as i64;
    if overflowed || (!negative && magnitude > u128::from(i64::MAX as u64)) {
        return (
            value,
            Some(format!(
                "Invalid quantity \"{raw}\": value is out of range, using overflow result for backwards compatibility"
            )),
        );
    }
    if cursor == end {
        return (value, None);
    }

    // Whitespace between the digits and the suffix is legal; `suffix_start` is where php-src's
    // `digits_end` lands after skipping it, and it is the slice point both diagnostics use.
    let mut suffix_start = cursor;
    while suffix_start < end && is_ini_space(bytes[suffix_start]) {
        suffix_start += 1;
    }
    let prefix = String::from_utf8_lossy(&bytes[..suffix_start]).into_owned();
    let last = bytes[end - 1];
    let factor: i64 = match last {
        b'g' | b'G' => 1024 * 1024 * 1024,
        b'm' | b'M' => 1024 * 1024,
        b'k' | b'K' => 1024,
        _ => {
            return (
                value,
                Some(format!(
                    "Invalid quantity \"{raw}\": unknown multiplier \"{}\", interpreting as \"{prefix}\" for backwards compatibility",
                    char::from(last)
                )),
            );
        }
    };
    let scaled = value.wrapping_mul(factor);
    if suffix_start < end - 1 {
        return (
            scaled,
            Some(format!(
                "Invalid quantity \"{raw}\", interpreting as \"{prefix}{}\" for backwards compatibility",
                char::from(last)
            )),
        );
    }
    (scaled, None)
}

/// Parses a SCANNER-REWRITTEN INI string as an integer directive value.
///
/// Every integer opcache directive but ONE is registered with php-src's generic long handler and
/// therefore reads its value through [`parse_ini_quantity`], which CANNOT FAIL — so this returns
/// `Some` and the caller always stores. VERIFIED on reference PHP 8.5.6 that
/// `opcache.max_file_size`, `opcache.file_update_protection`, `opcache.revalidate_freq`,
/// `opcache.log_verbosity_level` and `opcache.jit_buffer_size` all read `8M` as `8388608` and
/// `8K` as `8192`, i.e. the plain quantity.
///
/// THE ONE EXCEPTION is `opcache.memory_consumption`, whose handler does NOT use the quantity
/// parser at all: php-src reads it with `atoi`, treats the result as a MEBIBYTE count, and
/// REFUSES the store below the 8 MiB floor. VERIFIED on reference PHP 8.5.6
/// (`-d opcache.memory_consumption=<v>`, reading `opcache_get_configuration()` and `ini_get`):
///
/// | raw       | `directives` | `ini_get` |                                    |
/// |-----------|--------------|-----------|------------------------------------|
/// | `256`     | `268435456`  | `'256'`   | 256 MiB                            |
/// | `256M`    | `268435456`  | `'256M'`  | `atoi` stops at `M` → still 256    |
/// | `256K`    | `268435456`  | `'256K'`  | ditto — the suffix is IGNORED      |
/// | `1G`      | `134217728`  | `'128'`   | `atoi` = 1 → below the floor → refused |
/// | `4`       | `134217728`  | `'128'`   | below the floor → refused          |
/// | `0`       | `134217728`  | `'128'`   | refused                            |
/// | `garbage` | `134217728`  | `'128'`   | `atoi` = 0 → refused               |
///
/// The `256K → 268435456` and `1G → default` rows are what make this `atoi`, not a byte size: an
/// earlier revision routed `opcache.memory_consumption` through a `K`/`M`/`G` byte-size reader
/// and so reported `262144` for `256K` and `1073741824` for `1G` where reference PHP reports
/// 256 MiB and the untouched default.
#[allow(dead_code)]
fn parse_ini_int(name: &str, raw: &str) -> Option<i64> {
    if name == MEMORY_CONSUMPTION {
        let mebibytes = parse_ini_atoi(raw);
        // php-src's `if (memsize < 8) { ... return FAILURE; }` — the 8 MiB floor.
        if mebibytes < 8 {
            return None;
        }
        return mebibytes.checked_mul(1024 * 1024);
    }
    // `opcache.max_accelerated_files` is the SECOND `atoi`-read integer directive
    // (`OnUpdateMaxAcceleratedFiles`), so a `K`/`M`/`G` suffix or a `0x` prefix is NOT honored:
    // VERIFIED on reference PHP 8.5.6 that `-d opcache.max_accelerated_files=8K` and `=0x1000`
    // both leave the default 10000 in place (`atoi` reads 8 and 0, each below the 200 floor),
    // where the quantity parser would have read 8192 and 4096 and stored them.
    let value = if name == MAX_ACCELERATED_FILES {
        parse_ini_atoi(raw)
    } else {
        parse_ini_quantity(raw).0
    };
    // php-src's range validators REFUSE the store (they never clamp), so an out-of-range value
    // leaves the compiled-in default — which is what `None` means to the caller.
    if let Some((lo, hi)) = directive_int_range(name) {
        if value < lo || value > hi {
            return None;
        }
    }
    Some(value)
}

/// Returns the startup diagnostics reference PHP would print for the supplied `--ini` overrides
/// of `version_id`'s directive table, as complete message bodies without a severity prefix.
///
/// WHY THIS EXISTS. `zend_ini_parse_quantity` reports a malformed quantity through an `errstr`
/// that php-src's `zend_ini_parse_quantity_warn` turns into a startup
/// `Warning: Invalid "<directive>" setting. <body> in Unknown on line 0`. The value is STILL
/// STORED (see [`parse_ini_quantity`]), so the warning is the only signal that a value was
/// misread — dropping it would let `--ini opcache.max_file_size=12abc` silently compile a binary
/// reporting `12` for what the user wrote as `12abc`.
///
/// WHY AT COMPILE TIME. Reference PHP emits these while REGISTERING the INI entries at startup,
/// which is exactly when elephc consumes `--ini`: the directive values are baked into the binary,
/// so the compile IS the registration. Emitting them then is the faithful analogue, and it is
/// also the only point at which they are actionable — the compiled binary has no INI file to fix.
///
/// SCOPE: the QUANTITY diagnostics and the ten JIT RANGE diagnostics. The quantity ones matter
/// because their value is stored anyway, so the warning is the only signal. The range ones are
/// modelled because reference PHP prints them through `zend_error(E_WARNING, …)`, i.e.
/// unconditionally, and because they are the only refusals a user cannot otherwise attribute (the
/// off-by-one in three of their message bounds means the reverted value is genuinely surprising —
/// see [`directive_int_range`]). NOT modelled: the `zend_accel_error(ACCEL_LOG_WARNING, …)` lines
/// for `opcache.max_accelerated_files` / `opcache.interned_strings_buffer` / the
/// `opcache.memory_consumption` floor, which reference PHP itself suppresses at the default
/// `opcache.log_verbosity_level` of 1 (they appear only at `>= 2`, on a timestamped channel elephc
/// has no counterpart for) — those refusals stay silent here too, exactly as reference PHP's
/// default configuration renders them. `opcache.max_wasted_percentage` and an invalid
/// `opcache.jit` spelling are likewise silent in reference PHP. The runtime
/// `ELEPHC_INI_*` path emits nothing — it is an elephc extension with no reference counterpart
/// (see [`directive_runtime_overridable`]), and a compiled binary has no startup phase to warn in.
#[allow(dead_code)]
pub fn ini_override_warnings(version_id: u32, overrides: &[(String, String)]) -> Vec<String> {
    let mut warnings = Vec::new();
    if overrides.is_empty() {
        return warnings;
    }
    for (name, value) in opcache_directives(version_id) {
        if !matches!(value, DirectiveValue::Int(_)) {
            continue;
        }
        let Some(raw) = latest_override(overrides, name) else {
            continue;
        };
        let scanned = ini_scanner_value(raw);
        // THREE integer directives never reach `zend_ini_parse_quantity_warn` and so never carry a
        // quantity diagnostic: `opcache.memory_consumption` and `opcache.max_accelerated_files`
        // are `atoi`-read, and `opcache.jit_prof_threshold` is a `double` (`OnUpdateReal`) that
        // merely REPORTS as an int in the 8.2 profile. VERIFIED on reference PHP 8.5.6:
        // `-d opcache.max_accelerated_files=12abc` and `-d opcache.jit_prof_threshold=0.005` print
        // nothing, while `-d opcache.interned_strings_buffer=12abc` and
        // `-d opcache.max_file_size=12abc` both print the `unknown multiplier "c"` line.
        let quantity_read =
            name != MEMORY_CONSUMPTION && name != MAX_ACCELERATED_FILES && name != JIT_PROF_THRESHOLD;
        if quantity_read {
            if let (_, Some(body)) = parse_ini_quantity(scanned) {
                warnings.push(format!("Invalid \"{name}\" setting. {body}"));
            }
        }
        // The RANGE diagnostic, emitted AFTER the quantity one for the same directive because
        // php-src runs `zend_ini_parse_quantity_warn` first and only then tests the range — so a
        // value like `--ini opcache.jit_hot_func=999abc` produces BOTH lines, in this order
        // (VERIFIED on reference PHP 8.5.6). `parse_ini_int` returning `None` IS the refusal, so
        // the two cannot disagree about whether the value was kept.
        if parse_ini_int(name, scanned).is_none() {
            if let Some(body) = ini_range_warning(name) {
                warnings.push(body);
            }
        }
    }
    warnings
}

/// Converts a raw INI override string for directive `name` into the directive's typed,
/// normalized value, choosing the conversion from the `base` default value's TYPE (so the same
/// raw string parses as a bool, int, float, or string depending on the directive). Returns
/// `None` when the raw string does not parse to that type, in which case the caller keeps the
/// compiled-in default rather than crashing on a malformed override.
///
/// THE SCANNER RUNS FIRST. Every raw value passes through [`ini_scanner_value`] before the type
/// dispatch below sees it, because php-src's INI scanner rewrites the `on`/`true`/`yes` and
/// `off`/`false`/`no`/`none`/`null` barewords to `"1"`/`""` for EVERY directive, ahead of every
/// handler. The type rules below are therefore stated in terms of the REWRITTEN value.
///
/// Type rules:
/// - Bool: NEVER FAILS. `true`/`yes`/`on` (case-insensitive) or a non-zero `atoi` → `true`;
///   everything else, including `garbage` and the empty string, → `false` (see `parse_ini_bool`).
/// - Int: NEVER FAILS for the generic quantity directives — a malformed quantity stores its
///   leading numeric prefix (or `0`) and warns (see `parse_ini_quantity` /
///   [`ini_override_warnings`]). `opcache.memory_consumption` is the one exception and CAN
///   refuse the store (see `parse_ini_int`).
/// - Float: `opcache.max_wasted_percentage` is a PERCENT read with C `atoi` semantics and its own
///   range (see [`parse_ini_max_wasted_percentage`]); every other float directive
///   (`opcache.jit_prof_threshold`) is read with `zend_strtod` LEADING-PREFIX semantics and
///   therefore NEVER fails (see [`parse_ini_float_prefix`]).
/// - Str: taken verbatim. Because `DirectiveValue::Str` holds a `&'static str` (the table's
///   defaults are static), a user override — which is not static — is intern-leaked once with
///   `Box::leak`. The compiler is a short-lived one-shot process and the number of `--ini`
///   string overrides is tiny, so this leak is bounded and deliberate.
/// - `opcache.jit` is the one VALIDATED string directive: php-src's `OnUpdateJit` handler parses
///   the value before storing it and refuses the store on a parse failure, so an invalid spelling
///   leaves both `ini_get('opcache.jit')` and `opcache_get_configuration()` reporting the compiled
///   default. VERIFIED on reference PHP 8.5.6 (`-d opcache.jit=garbage` → `ini_get` = `'disable'`)
///   and 8.2.31 (same flag → `'tracing'`, that build's default). [`parse_jit_mode`] is that
///   predicate; note that the engine's `kind`/`opt_level`/`opt_flags` are NOT simply reset by the
///   rejection — see [`effective_jit_config`].
#[allow(dead_code)]
pub fn parse_ini_override(name: &str, raw: &str, base: &DirectiveValue) -> Option<DirectiveValue> {
    // php-src's INI SCANNER rewrites the boolean-alias barewords before any handler runs, for
    // every directive type. Doing it here — once, ahead of the dispatch — is what keeps the
    // typed value and the `ini_get` string (which re-applies the same rewrite in
    // `effective_directive_ini_string`) in agreement.
    let raw = ini_scanner_value(raw);
    // NAME-KEYED, ahead of BOTH numeric arms: `opcache.jit_prof_threshold` is a `double`
    // (`OnUpdateReal`) on every version, so it is always read with the `zend_strtod` prefix
    // semantics — but the 8.2 profile REPORTS it truncated to an int, which is exactly the base
    // value's variant. Reading it through the Int arm would apply the quantity parser instead and
    // report `0x10` as 16 where reference PHP 8.2 reports 0. See [`JIT_PROF_THRESHOLD`].
    if name == JIT_PROF_THRESHOLD {
        let parsed = parse_ini_float_prefix(raw);
        return Some(match base {
            // C's implicit `double` → `zend_long` conversion truncates toward zero.
            DirectiveValue::Int(_) => DirectiveValue::Int(parsed as i64),
            _ => DirectiveValue::Float(parsed),
        });
    }
    match base {
        DirectiveValue::Bool(_) => Some(DirectiveValue::Bool(parse_ini_bool(raw))),
        DirectiveValue::Int(_) => parse_ini_int(name, raw).map(DirectiveValue::Int),
        // NAME-KEYED, ahead of the generic Float arm: max_wasted_percentage's raw INI value is
        // a percent, not the fraction the table stores.
        DirectiveValue::Float(_) if name == MAX_WASTED_PERCENTAGE => {
            parse_ini_max_wasted_percentage(raw).map(DirectiveValue::Float)
        }
        DirectiveValue::Float(_) => Some(DirectiveValue::Float(parse_ini_float_prefix(raw))),
        DirectiveValue::Str(_) => {
            // `opcache.jit` is validated before it is stored (php-src `OnUpdateJit`); every other
            // string directive is stored verbatim.
            if name == JIT_DIRECTIVE && parse_jit_mode(raw).is_none() {
                return None;
            }
            let leaked: &'static str = Box::leak(raw.to_string().into_boxed_str());
            Some(DirectiveValue::Str(leaked))
        }
    }
}

/// The one float directive whose raw INI value is a percent rather than its normalized form.
const MAX_WASTED_PERCENTAGE: &str = "opcache.max_wasted_percentage";

/// Parses an `opcache.max_wasted_percentage` override from its RAW PERCENT form to the
/// normalized fraction `opcache_get_configuration()` reports.
///
/// Reference PHP registers this directive with `OnUpdateMaxWastedPercentage`, which reads the raw
/// string with C `atoi` — an INTEGER truncation, not a float parse — and refuses the store when
/// the result is `<= 0` or `> 50`, leaving the compiled default in place for both
/// `opcache_get_configuration()` and `ini_get()`.
///
/// VERIFIED on reference PHP 8.5.6 (`php -d opcache.enable=1 -d opcache.enable_cli=1
/// -d opcache.max_wasted_percentage=<v>`, reading both surfaces):
/// | raw      | directives | `ini_get` |
/// |----------|------------|-----------|
/// | `1`      | `0.01`     | `'1'`     |
/// | `2.5`    | `0.02`     | `'2.5'`   |
/// | `1.9`    | `0.01`     | `'1.9'`   |
/// | `50`     | `0.5`      | `'50'`    |
/// | `50.9`   | `0.5`      | `'50.9'`  |
/// | `2abc`   | `0.02`     | `'2abc'`  |
/// | `+3`     | `0.03`     | `'+3'`    |
/// | `3e1`    | `0.03`     | `'3e1'`   |
/// | ` 7 `    | `0.07`     | `' 7 '`   |
/// | `0.1`    | `0.05`     | `'5'`     |
/// | `0`/`-3` | `0.05`     | `'5'`     |
/// | `0x10`   | `0.05`     | `'5'`     |
/// | `abc`    | `0.05`     | `'5'`     |
///
/// The `2.5 → 0.02` and `3e1 → 0.03` rows are what make this `atoi`, not `zend_strtod`: a float
/// parse would give `0.025` and `0.3`. An earlier revision of this function used
/// `raw.trim().parse::<f64>()` and was wrong for every fractional and exponent-form percent.
#[allow(dead_code)]
fn parse_ini_max_wasted_percentage(raw: &str) -> Option<f64> {
    let percent = parse_ini_atoi(raw);
    if percent <= 0 || percent > 50 {
        return None;
    }
    Some(percent as f64 / 100.0)
}

/// Reads `raw` with C `atoi` semantics: skip leading ASCII whitespace, accept one optional
/// `+`/`-`, consume the leading run of decimal digits, and stop at the first non-digit. A string
/// with no leading digits yields `0`; digits beyond the 18th are dropped rather than overflowing
/// (a saturation the real `atoi` leaves undefined and that no plausible directive value reaches).
///
/// This is the reader `OnUpdateMaxWastedPercentage` uses, and it is deliberately NOT Rust's
/// `str::parse` (which rejects `2.5` and `2abc`) nor PHP's own `(int)` cast (which reads `3e1` as
/// `30`, whereas `atoi` reads `3`).
#[allow(dead_code)]
fn parse_ini_atoi(raw: &str) -> i64 {
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    let mut negative = false;
    if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
        negative = bytes[index] == b'-';
        index += 1;
    }
    let mut value: i64 = 0;
    let mut digits = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        if digits < 18 {
            value = value * 10 + i64::from(bytes[index] - b'0');
        }
        digits += 1;
        index += 1;
    }
    if negative {
        -value
    } else {
        value
    }
}

/// Reads a plain float directive (`opcache.jit_prof_threshold`) with `zend_strtod` LEADING-PREFIX
/// semantics: parse the longest prefix that forms a valid C float literal and yield `0.0` when
/// there is none. It therefore NEVER fails, which is why the caller wraps it in `Some(..)`
/// unconditionally.
///
/// VERIFIED on reference PHP 8.5.6 (`-d opcache.jit_prof_threshold=<v>`, reading
/// `opcache_get_configuration()` and `ini_get`): `0.5` → `0.5`, `1e-3` → `0.001`, `3` → `3.0`,
/// `-1` → `-1.0`, `0.005x` → `0.005` (prefix!), `abc` → `0.0`, `` (empty) → `0.0`. In every case
/// `ini_get` reports the raw string VERBATIM — the store always succeeds. An earlier revision
/// used `raw.trim().parse::<f64>().ok()`, which rejected `abc`, `` and `0.005x` and so kept the
/// compiled default `0.005` where reference PHP reports `0.0` / `0.005`.
#[allow(dead_code)]
fn parse_ini_float_prefix(raw: &str) -> f64 {
    let trimmed = raw.trim_start();
    let bytes = trimmed.as_bytes();
    let mut index = 0;
    if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
        index += 1;
    }
    let mantissa_start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
    }
    // No digit in the mantissa at all ⇒ no valid prefix ⇒ 0.0 (strtod's "no conversion").
    if index == mantissa_start || (index == mantissa_start + 1 && bytes[mantissa_start] == b'.') {
        return 0.0;
    }
    // An exponent counts only when it carries at least one digit; otherwise `1e` parses as `1`.
    if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
        let mut probe = index + 1;
        if probe < bytes.len() && (bytes[probe] == b'+' || bytes[probe] == b'-') {
            probe += 1;
        }
        let exponent_digits_start = probe;
        while probe < bytes.len() && bytes[probe].is_ascii_digit() {
            probe += 1;
        }
        if probe > exponent_digits_start {
            index = probe;
        }
    }
    let candidate = &trimmed[..index];
    // The scanned prefix is by construction a valid Rust float literal, so a parse failure can
    // only mean an out-of-range magnitude — which `strtod` reports as ±inf/0.0 rather than an
    // error, and which no directive value reaches. Fall back to 0.0 there.
    candidate.parse::<f64>().unwrap_or(0.0)
}

/// Returns the directive table for `version_id` with the supplied `--ini` overrides applied to
/// the typed/normalized values. Overrides are keyed by directive name; an override for an
/// unknown directive is ignored (only names already in the table are considered) and one that
/// fails to parse for the directive's type leaves the default in place. Repeated overrides for a
/// key are last-wins. This is the single effective table every OPcache consumer reads
/// (`opcache_get_configuration`, `opcache_get_status`, the enabled-state, and — via
/// `effective_directive_ini_string` — `ini_get`). With no overrides it is byte-identical to
/// `opcache_directives`.
#[allow(dead_code)]
pub fn effective_opcache_directives(
    version_id: u32,
    overrides: &[(String, String)],
) -> Vec<(&'static str, DirectiveValue)> {
    let mut directives = opcache_directives(version_id);
    if overrides.is_empty() {
        return directives;
    }
    for (name, value) in directives.iter_mut() {
        if let Some(raw) = latest_override(overrides, name) {
            if let Some(parsed) = parse_ini_override(name, raw, value) {
                *value = parsed;
            }
        }
    }
    directives
}

/// Returns the effective RAW INI STRING for directive `name` (what `ini_get('opcache.*')`
/// reports): the user's raw override string verbatim when the directive was validly overridden,
/// otherwise the default projection from `directive_ini_string`. `base_value` is the compiled-in
/// default, used only to decide whether the override parses for the directive's type — an
/// unparseable override falls back to the default string. Returning the user's raw string (not a
/// re-projection of the normalized value) matches reference PHP: `php -d opcache.memory_consumption=256`
/// makes `ini_get('opcache.memory_consumption')` report `"256"`, not `"256M"` or the byte count.
///
/// "THE USER'S RAW STRING" MEANS THE SCANNER-REWRITTEN ONE. What `ini_get()` echoes is the value
/// as php-src's INI scanner stored it, and the scanner rewrites the boolean-alias barewords for
/// every directive (see [`ini_scanner_value`]) — so `--ini opcache.preferred_memory_model=on`
/// reports `'1'`, not `'on'`. Everything else is echoed byte-for-byte, INCLUDING values the
/// directive's handler considered malformed but stored anyway:
/// `--ini opcache.max_file_size=12abc` stores `12` and reports `'12abc'`
/// (VERIFIED on reference PHP 8.5.6).
#[allow(dead_code)]
pub fn effective_directive_ini_string(
    name: &str,
    base_value: &DirectiveValue,
    overrides: &[(String, String)],
) -> String {
    if let Some(raw) = latest_override(overrides, name) {
        if parse_ini_override(name, raw, base_value).is_some() {
            return ini_scanner_value(raw).to_string();
        }
    }
    directive_ini_string(name, base_value)
}

/// The `ELEPHC_INI_` prefix every runtime per-directive environment override carries. It joins
/// the existing `ELEPHC_*` runtime knobs (`ELEPHC_NULL_REPR`, `ELEPHC_SESSION_AUTO_START`,
/// `ELEPHC_TLS_LIB_DIR`).
///
/// NON-PARITY, DELIBERATELY: reference PHP has NO per-directive environment override. Its only
/// environment mechanisms are FILE-granularity (`PHPRC`, `PHP_INI_SCAN_DIR`) — VERIFIED on
/// reference PHP 8.5.6, where `PHP_INI_opcache_jit=tracing`, `opcache_jit=tracing` and
/// `opcache.jit=tracing` in the environment all leave `ini_get('opcache.jit')` at the compiled
/// default. This is elephc's answer to `-d` for an AOT binary whose php.ini is compiled in.
#[allow(dead_code)]
pub const RUNTIME_INI_ENV_PREFIX: &str = "ELEPHC_INI_";

/// Returns the two environment-variable spellings that override directive `name` at runtime:
/// `(primary, secondary)`.
///
/// The PRIMARY spelling replaces each `.` with `__` (`opcache.jit` →
/// `ELEPHC_INI_opcache__jit`) because POSIX shells reject a dot in an assignment word — `FOO.BAR=1
/// cmd` is a syntax error in `sh`/`bash`/`zsh`, so the dotted form is unusable from a command
/// line. The SECONDARY spelling keeps the literal dot (`ELEPHC_INI_opcache.jit`) and is consulted
/// ONLY when the primary is unset/empty; it stays reachable through `env`, `putenv`, Docker
/// `--env`, and systemd unit files, all of which accept dots.
///
/// The directive part is kept VERBATIM (lowercase), not upper-cased: upper-casing would fold
/// `session.upload_progress.min_freq`-style names into ambiguity the moment this family extends
/// past `opcache.*`, and a lowercase environment name is legal everywhere it matters.
#[allow(dead_code)]
pub fn directive_env_var_names(name: &str) -> (String, String) {
    (
        format!("{RUNTIME_INI_ENV_PREFIX}{}", name.replace('.', "__")),
        format!("{RUNTIME_INI_ENV_PREFIX}{name}"),
    )
}

/// Whether directive `name` may be overridden at RUNTIME through its `ELEPHC_INI_*` environment
/// variable. `true` for the reporting-only majority; `false` for the ten directives elephc
/// DERIVES compiled-in behavior from.
///
/// THE SCOPE RULE: a runtime override is honored only where honoring it cannot make the binary
/// contradict itself. Every excluded directive below is consumed at COMPILE TIME to bake code or
/// baked constants that a runtime environment variable cannot retroactively change, so reporting
/// a new value for it would produce a LYING binary — `ini_get('opcache.enable_cli') === '1'` next
/// to an `opcache_get_status()` that still returns `false`. An ignored environment variable is
/// honest; a self-contradicting report is not.
///
/// The excluded set and the compile-time consumer that forces each exclusion:
/// - `opcache.enable`, `opcache.enable_cli` — `crate::opcache::state::opcache_cache_enabled_with_overrides`
///   bakes the enabled gate as a literal `false === false` in `opcache_get_status`,
///   `opcache_reset`, `opcache_invalidate`, `opcache_is_script_cached` and `opcache_compile_file`.
/// - `opcache.memory_consumption`, `opcache.interned_strings_buffer`,
///   `opcache.max_accelerated_files`, `opcache.revalidate_freq` — read by
///   `crate::opcache_prelude`'s `directive_int` to bake the `opcache_get_status()` memory /
///   interned-string / cached-key arithmetic and the `scripts` map's `revalidate` field.
/// - `opcache.jit`, `opcache.jit_buffer_size` — [`effective_jit_config`] bakes the
///   `opcache_get_status()['jit']` `kind`/`opt_level`/`opt_flags` triple from them.
/// - `opcache.restrict_api` — decided at compile time (`restrict_api_denies`) to select the
///   RESTRICTED function bodies; the choice of body cannot be revisited at runtime.
/// - `opcache.preload` — decided at compile time (`preload_verdict`); it can FAIL THE COMPILE and
///   otherwise bakes the `preload_statistics` block.
///
/// Everything else is projected straight through to `opcache_get_configuration()['directives']`
/// and `ini_get()`/`ini_get_all()` and nowhere else, so a runtime override of it is fully
/// consistent.
#[allow(dead_code)]
pub fn directive_runtime_overridable(name: &str) -> bool {
    !matches!(
        name,
        "opcache.enable"
            | "opcache.enable_cli"
            | "opcache.memory_consumption"
            | "opcache.interned_strings_buffer"
            | "opcache.max_accelerated_files"
            | "opcache.revalidate_freq"
            | "opcache.jit"
            | "opcache.jit_buffer_size"
            | "opcache.restrict_api"
            | "opcache.preload"
    )
}

/// Returns the one-character TYPE CODE the baked PHP normalizer switches on for directive `name`
/// carrying default `value`. It is the runtime mirror of the type dispatch [`parse_ini_override`]
/// performs in Rust at compile time:
///
/// - `'b'` — bool (`parse_ini_bool`; never fails)
/// - `'i'` — int (`parse_ini_quantity`: decimal / `0x` hex / `0b` binary / leading-`0` octal,
///   `K`/`M`/`G` suffixes; never fails). `opcache.memory_consumption`, the one integer directive
///   with `atoi`-mebibyte semantics that CAN refuse a value, is not runtime-overridable at all
///   (see [`directive_runtime_overridable`]), so no separate code is needed for it.
/// - `'p'` — the `opcache.max_wasted_percentage` PERCENT (`parse_ini_max_wasted_percentage`)
/// - `'f'` — plain float (`parse_ini_float_prefix`)
/// - `'t'` — `opcache.jit_prof_threshold` in the 8.2 profile ONLY: a float READ that is REPORTED
///   truncated to an int (see [`JIT_PROF_THRESHOLD`]). It is its own code rather than `'i'`
///   because the quantity parser the `'i'` normalizer runs would report `0x10` as 16 where
///   reference PHP 8.2 reports 0, and its own code rather than `'f'` because the reported value
///   must come out an int. On 8.3+ the same directive is a plain `'f'`.
/// - `'s'` — string, stored verbatim
///
/// The percent and truncating-float codes are name-keyed ahead of the generic arms exactly as
/// `parse_ini_override`'s match arms are, so the two dispatches cannot drift.
#[allow(dead_code)]
pub fn directive_env_type_code(name: &str, value: &DirectiveValue) -> char {
    match value {
        // NAME-KEYED FIRST, mirroring `parse_ini_override`'s pre-match arm.
        _ if name == JIT_PROF_THRESHOLD => match value {
            DirectiveValue::Int(_) => 't',
            _ => 'f',
        },
        DirectiveValue::Bool(_) => 'b',
        DirectiveValue::Int(_) => 'i',
        DirectiveValue::Float(_) if name == MAX_WASTED_PERCENTAGE => 'p',
        DirectiveValue::Float(_) => 'f',
        DirectiveValue::Str(_) => 's',
    }
}

/// Returns whether directive `name` is registered by php-src with a C `NULL` default rather than
/// an empty string — the ONE thing that makes `ini_get_all()` report its `global_value` /
/// `local_value` as PHP `null` instead of `''`.
///
/// `opcache.file_cache` is the only such directive in the whole 54-entry block. VERIFIED on
/// reference PHP 8.5.6 by scanning every `opcache.*` entry of `ini_get_all()` for a NULL on either
/// side: exactly one hit.
///
/// ```text
/// $ php -d xdebug.mode=off -d opcache.enable=1 -d opcache.enable_cli=1 \
///     -r 'foreach (ini_get_all() as $k => $v) { if (strncmp($k, "opcache.", 8)) continue;
///         if ($v["global_value"] === null || $v["local_value"] === null) echo "$k\n"; }'
/// opcache.file_cache
/// ```
///
/// php-src registers it as `STD_PHP_INI_ENTRY("opcache.file_cache", NULL, …, OnUpdateFileCache,
/// …)`; every other opcache string directive uses `""`.
///
/// THE NULL IS "NEVER SET", NOT "EMPTY". Setting the directive to the empty string makes reference
/// PHP report `string(0) ""` on both sides, not `NULL` — VERIFIED: `-d opcache.file_cache=` yields
/// `''`/`''`, `-d opcache.file_cache=/tmp/fcx` yields `'/tmp/fcx'`/`'/tmp/fcx'`, and only the
/// unconfigured run yields `NULL`/`NULL`. `ini_get('opcache.file_cache')` reports `string(0) ""`
/// in ALL THREE cases — the NULL is visible through `ini_get_all()` alone, which is why this
/// predicate is consulted only there.
/// `allow(dead_code)`: dead in the `elephc-magician` `#[path]` include (which does not build the
/// `ini_get_all` surface), live in `elephc`.
#[allow(dead_code)]
pub fn directive_ini_null_default(name: &str) -> bool {
    name == "opcache.file_cache"
}

/// Returns the `PHP_INI_*` access-level bitmask reference PHP reports under
/// `ini_get_all()[<dir>]['access']` for the opcache directive `name`.
///
/// Values follow the standard PHP constants: `PHP_INI_USER = 1`, `PHP_INI_PERDIR = 2`,
/// `PHP_INI_SYSTEM = 4`, `PHP_INI_ALL = 7`. Every opcache directive is either
/// `PHP_INI_SYSTEM` (4, the majority) or `PHP_INI_ALL` (7).
///
/// THE COUNT IS 18, not 19. The `matches!` arm below lists NINETEEN names, but one of them —
/// `opcache.consistency_checks` — exists only in the 8.2 directive set, so no single target ever
/// sees more than 18 `PHP_INI_ALL` directives. VERIFIED on reference PHP 8.5.6:
/// `count(array_filter(ini_get_all(), fn($v) => $v['access'] === 7))` restricted to `opcache.*`
/// is 18 out of 54, and the 18 names are exactly this list minus `opcache.consistency_checks`.
/// `opcache.consistency_checks` is `PHP_INI_ALL` per the php-src 8.2 registration
/// (`OnUpdateConsistencyChecks`).
/// The access level of a directive does not vary across the maintained versions (only the
/// directive *set* does), so a single name-keyed lookup is correct for all of 8.2–8.5.
/// `allow(dead_code)`: dead in the `elephc-magician` `#[path]` include (which does not build
/// the `ini_get_all` access surface), live in `elephc`.
#[allow(dead_code)]
pub fn directive_access(name: &str) -> u8 {
    // PHP_INI_ALL (7) directives; every other opcache directive is PHP_INI_SYSTEM (4).
    let php_ini_all = matches!(
        name,
        "opcache.enable"
            | "opcache.dups_fix"
            | "opcache.revalidate_path"
            | "opcache.validate_timestamps"
            | "opcache.revalidate_freq"
            | "opcache.file_update_protection"
            | "opcache.consistency_checks"
            | "opcache.jit"
            | "opcache.jit_debug"
            | "opcache.jit_bisect_limit"
            | "opcache.jit_blacklist_root_trace"
            | "opcache.jit_blacklist_side_trace"
            | "opcache.jit_hot_side_exit"
            | "opcache.jit_max_loop_unrolls"
            | "opcache.jit_max_polymorphic_calls"
            | "opcache.jit_max_recursive_calls"
            | "opcache.jit_max_recursive_returns"
            | "opcache.jit_max_trace_length"
            | "opcache.jit_prof_threshold"
    );
    if php_ini_all {
        7
    } else {
        4
    }
}

/// The `opcache.jit` directive name.
const JIT_DIRECTIVE: &str = "opcache.jit";

// ---------------------------------------------------------------------------------------------
// `opcache.jit` mode parsing (php-src `zend_jit_config` / `zend_jit_parse_config_num`)
// ---------------------------------------------------------------------------------------------

/// php-src `ZEND_JIT_ON_SCRIPT_LOAD` — compile every function when the script is loaded. This is
/// the trigger the `function` spelling selects, so `function` reports `kind = 0`.
const JIT_TRIGGER_ON_SCRIPT_LOAD: i64 = 0;

/// php-src `ZEND_JIT_ON_HOT_TRACE` — the tracing JIT. This is the trigger `tracing` (and its
/// aliases `on`/`yes`/`true`/`1`) selects, so they report `kind = 5`.
const JIT_TRIGGER_ON_HOT_TRACE: i64 = 5;

/// php-src `ZEND_JIT_ON_DOC_COMMENT` — the `@jit` doc-comment trigger. The constant still occupies
/// slot 4 in the trigger numbering, but a CRTO spelling with `T = 4` is REJECTED (verified on both
/// reference builds, see [`apply_jit_config_num`]); it exists here only to name the hole.
const JIT_TRIGGER_ON_DOC_COMMENT: u64 = 4;

/// The highest accepted CRTO trigger digit (`ZEND_JIT_ON_HOT_TRACE`).
const JIT_TRIGGER_MAX: u64 = 5;

/// php-src `ZEND_JIT_LEVEL_OPT_FUNCS` — optimize using inter-function analysis. The `opt_level`
/// the `tracing` spelling selects (`tracing` is the alias of the CRTO form `1254`).
const JIT_OPT_LEVEL_OPT_FUNCS: i64 = 4;

/// php-src `ZEND_JIT_LEVEL_OPT_SCRIPT` — optimize using whole-script analysis. The `opt_level`
/// the `function` spelling selects (`function` is the alias of the CRTO form `1205`), and the
/// highest accepted CRTO `O` digit.
const JIT_OPT_LEVEL_OPT_SCRIPT: i64 = 5;

/// php-src `ZEND_JIT_REG_ALLOC_LOCAL` (1 << 0) — the `R = 1` register-allocation mode.
const JIT_REG_ALLOC_LOCAL: i64 = 1;

/// php-src `ZEND_JIT_REG_ALLOC_GLOBAL` (1 << 1) — the `R = 2` register-allocation mode.
const JIT_REG_ALLOC_GLOBAL: i64 = 2;

/// php-src `ZEND_JIT_CPU_REG_ALLOC` (1 << 2) — set by the CRTO `C = 1` digit (use CPU-specific
/// optimizations). `tracing`/`function` both set it, which is why both report `opt_flags = 6`
/// (`ZEND_JIT_REG_ALLOC_GLOBAL | ZEND_JIT_CPU_REG_ALLOC`).
const JIT_CPU_REG_ALLOC: i64 = 4;

/// The JIT engine state an `opcache.jit` spelling produces — php-src's `JIT_G(enabled)`,
/// `JIT_G(on)`, `JIT_G(trigger)`, `JIT_G(opt_level)` and `JIT_G(opt_flags)`, which are exactly
/// the five values `opcache_get_status()['jit']` reports as `enabled` / `on` / `kind` /
/// `opt_level` / `opt_flags`.
///
/// It is a MUTABLE STATE, not a pure parse result, because php-src's setting handler mutates a
/// process-global in place and only assigns the fields its arm reaches — which is observable
/// (see [`apply_jit_setting`] and [`effective_jit_config`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct JitConfig {
    /// `JIT_G(enabled)` — the JIT was configured (as opposed to `disable`d outright).
    pub enabled: bool,
    /// `JIT_G(on)` — the JIT is configured to actually compile something.
    pub on: bool,
    /// `JIT_G(trigger)` — WHEN functions/traces get compiled; the CRTO `T` digit.
    pub kind: i64,
    /// `JIT_G(opt_level)` — the optimization level; the CRTO `O` digit.
    pub opt_level: i64,
    /// `JIT_G(opt_flags)` — the register-allocation/CPU bitmask; composed from `R` and `C`.
    pub opt_flags: i64,
}

impl JitConfig {
    /// The zero-initialized engine state, matching php-src's `JIT_G` before any
    /// `opcache.jit` value has been processed.
    #[allow(dead_code)]
    pub const ZERO: JitConfig = JitConfig {
        enabled: false,
        on: false,
        kind: 0,
        opt_level: 0,
        opt_flags: 0,
    };
}

/// Applies ONE `opcache.jit` spelling to `state` exactly the way php-src's `zend_jit_config()`
/// mutates `JIT_G`, returning whether the spelling was ACCEPTED. A rejected spelling still leaves
/// behind whatever the partial parse already assigned — that residue is observable (see below),
/// so this deliberately takes `&mut` rather than returning a fresh value.
///
/// ACCEPTED SPELLINGS (all keyword matches are ASCII case-insensitive, matching php-src's
/// `zend_string_equals_literal_ci`):
///
/// | spelling                              | enabled | on    | kind | opt_level | opt_flags |
/// |---------------------------------------|---------|-------|------|-----------|-----------|
/// | `disable`                             | false   | false | kept | kept      | kept      |
/// | `off` / `no` / `false` / `0` / empty  | true    | false | kept | kept      | kept      |
/// | `on` / `yes` / `true` / `1` / `tracing` | true  | true  | 5    | 4         | 6         |
/// | `function`                            | true    | true  | 0    | 5         | 6         |
/// | CRTO number (see [`apply_jit_config_num`]) | true | true | `T` | `O`      | f(`R`,`C`) |
///
/// `tracing` is the documented alias of the CRTO form `1254` and `function` of `1205`; the table
/// above is simply those two numbers decoded, and both were confirmed to agree digit-for-digit.
///
/// "kept" is load-bearing: the `disable` and `off` arms do NOT reset `kind`/`opt_level`/
/// `opt_flags`. That is what makes a failed override's residue visible on an 8.4/8.5 target,
/// whose compiled default is `disable` (see [`effective_jit_config`]).
///
/// NOTE on the literal `0`/`1` arms: reference PHP's INI SCANNER rewrites the generic boolean
/// aliases before the handler ever sees them (`on`/`yes`/`true` → `"1"`, `off`/`no`/`false` →
/// `""`), which is why `php -d opcache.jit=on` reports `ini_get('opcache.jit') === '1'`. That
/// rewrite is generic to every PHP INI string directive, not an `opcache.jit` rule, and elephc's
/// `--ini` does not model it — so this function accepts BOTH the raw words and their rewritten
/// forms, and the word spellings reach it verbatim. `"1"` must therefore be matched as a KEYWORD
/// (→ tracing) ahead of the numeric path, which would otherwise decode it as `C=0 R=0 T=0 O=1`;
/// reference PHP does the same (verified: `-d opcache.jit=1` reports kind 5, opt_level 4,
/// opt_flags 6, while `-d opcache.jit=0001` reports kind 0, opt_level 1, opt_flags 0).
///
/// VERIFIED against reference PHP 8.5.6 and 8.2.31 (Homebrew, macOS arm64) with
/// `php -d opcache.enable=1 -d opcache.enable_cli=1 -d opcache.jit_buffer_size=64M
/// -d opcache.jit=<spelling> -r 'var_export(opcache_get_status()["jit"]);'`.
#[allow(dead_code)]
pub fn apply_jit_setting(state: &mut JitConfig, spelling: &str) -> bool {
    // `disable`: the JIT is not configured at all. Leaves the tuning fields untouched.
    if spelling.eq_ignore_ascii_case("disable") {
        state.enabled = false;
        state.on = false;
        return true;
    }
    // The "configured but switched off" arm. `""` is the INI scanner's rewrite of
    // `off`/`no`/`false`/`none`/`null`, which is how every `--ini` override reaches this
    // function (see `ini_scanner_value`); the words are also accepted directly so a caller that
    // hands over an un-rewritten spelling — the compiled DEFAULT below, or the eval interpreter
    // — still maps it the same way.
    if spelling.is_empty()
        || spelling == "0"
        || spelling.eq_ignore_ascii_case("off")
        || spelling.eq_ignore_ascii_case("no")
        || spelling.eq_ignore_ascii_case("false")
    {
        state.enabled = true;
        state.on = false;
        return true;
    }
    // The tracing JIT. `"1"` is the INI scanner's rewrite of `on`/`yes`/`true` and therefore the
    // form a `--ini opcache.jit=on` override actually arrives in; all of them alias the CRTO
    // form `1254`. VERIFIED on reference PHP 8.5.6: `-d opcache.jit=on` reports
    // `ini_get('opcache.jit') === '1'` with `opcache_get_status()['jit']` kind 5 / opt_level 4 /
    // opt_flags 6 — the plain `tracing` triple.
    if spelling == "1"
        || spelling.eq_ignore_ascii_case("on")
        || spelling.eq_ignore_ascii_case("yes")
        || spelling.eq_ignore_ascii_case("true")
        || spelling.eq_ignore_ascii_case("tracing")
    {
        state.enabled = true;
        state.on = true;
        state.kind = JIT_TRIGGER_ON_HOT_TRACE;
        state.opt_level = JIT_OPT_LEVEL_OPT_FUNCS;
        state.opt_flags = JIT_REG_ALLOC_GLOBAL | JIT_CPU_REG_ALLOC;
        return true;
    }
    // The function JIT — the alias of the CRTO form `1205`.
    if spelling.eq_ignore_ascii_case("function") {
        state.enabled = true;
        state.on = true;
        state.kind = JIT_TRIGGER_ON_SCRIPT_LOAD;
        state.opt_level = JIT_OPT_LEVEL_OPT_SCRIPT;
        state.opt_flags = JIT_REG_ALLOC_GLOBAL | JIT_CPU_REG_ALLOC;
        return true;
    }
    match parse_jit_config_num(spelling) {
        Some(num) => apply_jit_config_num(state, num),
        None => false,
    }
}

/// Parses the numeric CRTO body of an `opcache.jit` spelling, or `None` when it is not a plain
/// number. Mirrors php-src's `ZEND_STRTOUL` + "the parse must consume the whole string" check:
/// an optional leading `+` and ASCII digits only, so `1254`, `+1254` and `01254` all parse to
/// 1254 (VERIFIED: all three are accepted by reference PHP and report kind 5 / opt_level 4 /
/// opt_flags 6) while `garbage`, `-1` and `1254 ` do not.
///
/// DOCUMENTED NARROWING: reference PHP's `strtoul` also skips LEADING whitespace, so a value
/// like `" 1254"` would be accepted there and is rejected here. Trailing whitespace is rejected
/// by both (the whole-string check). A `--ini` value with surrounding spaces is not a spelling
/// any real configuration uses, and rejecting it merely falls back to the compiled default.
///
/// No upper bound is applied: a number wider than four digits is caught by the `C` digit check
/// in [`apply_jit_config_num`] (`12540 / 1000 = 12 > 1`), exactly as php-src's own digit checks
/// catch it. An integer too large for `u64` fails to parse and is rejected.
fn parse_jit_config_num(spelling: &str) -> Option<u64> {
    let digits = spelling.strip_prefix('+').unwrap_or(spelling);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u64>().ok()
}

/// Applies a numeric CRTO `opcache.jit` value to `state` the way php-src's
/// `zend_jit_parse_config_num()` does, returning whether it was accepted.
///
/// The four digits are `CRTO`, most significant first:
/// - `C` (thousands) — use CPU-specific optimizations. Accepted range `0..=1`; `1` ORs
///   `ZEND_JIT_CPU_REG_ALLOC` (4) into `opt_flags`.
/// - `R` (hundreds) — register allocation. Accepted range `0..=2`; `0` → no bit, `1` →
///   `ZEND_JIT_REG_ALLOC_LOCAL` (1), `2` → `ZEND_JIT_REG_ALLOC_GLOBAL` (2). ASSIGNS `opt_flags`.
/// - `T` (tens) — the trigger, reported verbatim as `kind`. Accepted values `0`, `1`, `2`, `3`
///   and `5`; `4` (`ZEND_JIT_ON_DOC_COMMENT`) is REJECTED.
/// - `O` (units) — the optimization level, reported verbatim as `opt_level`. Accepted range
///   `1..=5`; `0` is rejected for any non-zero number.
///
/// `0` (in any zero-padded spelling) is the special "configured but switched off" value: it
/// returns success having set only `enabled`/`on`, identical to the `off` keyword arm.
///
/// THE ASSIGN/VALIDATE INTERLEAVING IS OBSERVABLE, so it is reproduced exactly: each digit is
/// validated and then assigned before the next digit is even looked at. A rejected value
/// therefore leaves a PARTIAL residue in `state`. VERIFIED on reference PHP 8.5.6 (whose
/// compiled default `disable` does not overwrite the residue):
/// - `1256` (`O = 6`) → kind 0, opt_level 0, opt_flags 0 — rejected before anything is assigned.
/// - `1244` (`T = 4`) → kind 0, opt_level 4, opt_flags 0 — `O` assigned, `T` rejected.
/// - `1354` (`R = 3`) → kind 5, opt_level 4, opt_flags 0 — `O`/`T` assigned, `R` rejected.
/// - `2254` (`C = 2`) → kind 5, opt_level 4, opt_flags 2 — `O`/`T`/`R` assigned, `C` rejected.
///
/// `enabled`/`on` are set LAST, only on success. Whether reference PHP sets them earlier is
/// unobservable: a rejected value is always followed by the compiled default being re-applied,
/// which rewrites both fields either way (see [`effective_jit_config`]).
fn apply_jit_config_num(state: &mut JitConfig, num: u64) -> bool {
    if num == 0 {
        // Same arm as the `off` keyword: configured, switched off, tuning fields untouched.
        state.enabled = true;
        state.on = false;
        return true;
    }

    // O — optimization level. `0` is rejected here (it is only legal as the whole value `0`).
    let opt_level = num % 10;
    if opt_level == 0 || opt_level > JIT_OPT_LEVEL_OPT_SCRIPT as u64 {
        return false;
    }
    state.opt_level = opt_level as i64;

    // T — trigger. `4` is the retired doc-comment trigger and is rejected.
    let trigger = (num / 10) % 10;
    if trigger > JIT_TRIGGER_MAX || trigger == JIT_TRIGGER_ON_DOC_COMMENT {
        return false;
    }
    state.kind = trigger as i64;

    // R — register allocation. ASSIGNS opt_flags (it does not OR into it).
    let reg_alloc = (num / 100) % 10;
    if reg_alloc > 2 {
        return false;
    }
    state.opt_flags = match reg_alloc {
        0 => 0,
        1 => JIT_REG_ALLOC_LOCAL,
        _ => JIT_REG_ALLOC_GLOBAL,
    };

    // C — CPU-specific optimizations. Also rejects any value wider than four digits.
    let cpu = num / 1000;
    if cpu > 1 {
        return false;
    }
    if cpu == 1 {
        state.opt_flags |= JIT_CPU_REG_ALLOC;
    }

    state.enabled = true;
    state.on = true;
    true
}

/// Returns the [`JitConfig`] a single `opcache.jit` spelling produces from a pristine engine
/// state, or `None` when the spelling is INVALID. This is the validity predicate
/// [`parse_ini_override`] uses to decide whether an `--ini opcache.jit=…` override is stored at
/// all; use [`effective_jit_config`] to get the config a compile target actually reports, which
/// also models what happens to an invalid override.
#[allow(dead_code)]
pub fn parse_jit_mode(spelling: &str) -> Option<JitConfig> {
    let mut state = JitConfig::ZERO;
    if apply_jit_setting(&mut state, spelling) {
        Some(state)
    } else {
        None
    }
}

/// Returns the JIT engine state a compile target reports for `version_id` with `overrides`
/// applied — reproducing php-src's INI-REGISTRATION TWO-PASS, which is what makes an invalid
/// `opcache.jit` value behave the way it does.
///
/// php-src's `zend_register_ini_entry_ex` offers a `-d`/ini-file value to the directive's
/// handler first; if the handler REJECTS it, the entry falls back to the COMPILED DEFAULT and
/// the handler is invoked a SECOND time with that default. So an invalid `opcache.jit`:
/// 1. emits `Warning: Invalid "opcache.jit" setting. Should be "disable", "on", "off",
///    "tracing", "function" or 4-digit number` at startup,
/// 2. leaves `ini_get('opcache.jit')` and `opcache_get_configuration()` reporting the COMPILED
///    DEFAULT (which is why [`parse_ini_override`] refuses to store it), and
/// 3. leaves the engine running the compiled default — applied ON TOP of whatever residue the
///    failed parse assigned (see [`apply_jit_config_num`]).
///
/// Step 3 has a per-version consequence, and BOTH halves were verified on a real build:
/// - 8.4/8.5 (default `disable`, which does not touch the tuning fields): the residue SURVIVES.
///   `php8.5 -d opcache.jit=1355` reports kind 5, opt_level 5, opt_flags 0 with `ini_get` =
///   `'disable'`.
/// - 8.2/8.3 (default `tracing`, which assigns all three): the residue is OVERWRITTEN.
///   `php8.2 -d opcache.jit=1355` reports kind 5, opt_level 4, opt_flags 6 — the plain `tracing`
///   values — with `ini_get` = `'tracing'`.
///
/// The same two-pass runs when there is no override at all (the handler is simply invoked once
/// with the compiled default), so this one function covers every case.
#[allow(dead_code)]
pub fn effective_jit_config(version_id: u32, overrides: &[(String, String)]) -> JitConfig {
    let default_spelling = opcache_directives(version_id)
        .into_iter()
        .find(|(name, _)| *name == JIT_DIRECTIVE)
        .and_then(|(_, value)| match value {
            DirectiveValue::Str(spelling) => Some(spelling),
            _ => None,
        })
        .unwrap_or("disable");

    let mut state = JitConfig::ZERO;
    if let Some(raw) = latest_override(overrides, JIT_DIRECTIVE) {
        // Through the INI scanner first, exactly as `parse_ini_override` does: `on` reaches the
        // handler as `"1"` (still TRACING) and `off`/`none`/`null` as `""`.
        if apply_jit_setting(&mut state, ini_scanner_value(raw)) {
            return state;
        }
        // Rejected: fall through so the compiled default is applied on top of the residue,
        // exactly as php-src's second handler invocation does.
    }
    apply_jit_setting(&mut state, default_spelling);
    state
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Guards the byte-verified 8.5 directive snapshot and the per-version deltas
    //! against accidental edits.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness (both crates that include this file).
    //!
    //! Key details:
    //! - The 8.5 count and representative values are pinned to the reference PHP 8.5.6
    //!   `opcache_get_configuration()['directives']` capture.

    use super::*;

    /// The 8.5 default set has exactly 54 directives (reference PHP 8.5.6).
    #[test]
    fn php85_directive_count_matches_reference() {
        assert_eq!(opcache_directives(80500).len(), 54);
    }

    /// Representative 8.5 normalized values match the reference capture.
    #[test]
    fn php85_representative_values_match_reference() {
        let directives = opcache_directives(80500);
        let value = |key: &str| {
            directives
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| *value)
        };
        assert_eq!(value("opcache.enable"), Some(DirectiveValue::Bool(true)));
        assert_eq!(value("opcache.enable_cli"), Some(DirectiveValue::Bool(false)));
        assert_eq!(
            value("opcache.memory_consumption"),
            Some(DirectiveValue::Int(134_217_728))
        );
        assert_eq!(
            value("opcache.max_wasted_percentage"),
            Some(DirectiveValue::Float(0.05))
        );
        assert_eq!(
            value("opcache.optimization_level"),
            Some(DirectiveValue::Int(2_147_401_727))
        );
        assert_eq!(value("opcache.jit"), Some(DirectiveValue::Str("disable")));
        assert_eq!(
            value("opcache.jit_buffer_size"),
            Some(DirectiveValue::Int(67_108_864))
        );
        assert_eq!(value("opcache.jit_hot_loop"), Some(DirectiveValue::Int(61)));
    }

    /// Per-version directive-set deltas hold: 8.2 has `consistency_checks` and no
    /// `jit_max_trace_length`; 8.5 has `file_cache_read_only`; JIT defaults flip.
    #[test]
    fn per_version_deltas_hold() {
        let has = |version: u32, key: &str| {
            opcache_directives(version)
                .iter()
                .any(|(name, _)| *name == key)
        };
        assert!(has(80200, "opcache.consistency_checks"));
        assert!(!has(80300, "opcache.consistency_checks"));
        assert!(!has(80200, "opcache.jit_max_trace_length"));
        assert!(has(80300, "opcache.jit_max_trace_length"));
        assert!(!has(80400, "opcache.file_cache_read_only"));
        assert!(has(80500, "opcache.file_cache_read_only"));

        let jit = |version: u32| {
            opcache_directives(version)
                .into_iter()
                .find(|(name, _)| *name == "opcache.jit")
                .map(|(_, value)| value)
        };
        assert_eq!(jit(80300), Some(DirectiveValue::Str("tracing")));
        assert_eq!(jit(80400), Some(DirectiveValue::Str("disable")));
    }

    /// `opcache.jit_prof_threshold` is reported as an INT by 8.2 and as a FLOAT by 8.3+.
    ///
    /// VERIFIED on real PHP 8.2.31 (`/opt/homebrew/opt/php@8.2/bin/php -d xdebug.mode=off
    /// -d opcache.enable=1 -d opcache.enable_cli=1`) → `int(0)` for the default and `'0.005'` from
    /// `ini_get`, against PHP 8.5.6 → `float(0.005)` and the same `'0.005'`. 8.3 CANNOT be probed
    /// on this host (`php@8.3` is a symlink to 8.5) and is DERIVED FROM php-src: the `PHP-8.2`
    /// branch of `ext/opcache/zend_accelerator_module.c` reports it with `add_assoc_long` while
    /// `PHP-8.3`, `PHP-8.4` and `PHP-8.5` all use `add_assoc_double`. See [`JIT_PROF_THRESHOLD`].
    #[test]
    fn jit_prof_threshold_is_an_int_only_in_the_82_profile() {
        let threshold = |version: u32| base(version, JIT_PROF_THRESHOLD);
        assert_eq!(threshold(80200), DirectiveValue::Int(0));
        for version in [80300u32, 80400, 80500] {
            assert_eq!(threshold(version), DirectiveValue::Float(0.005));
        }
        // The RAW INI string is `'0.005'` on every version — it is the registered INI default, not
        // a projection of the normalized value.
        for version in [80200u32, 80300, 80400, 80500] {
            assert_eq!(ini(version, JIT_PROF_THRESHOLD), "0.005");
        }
    }

    /// An 8.2 `--ini opcache.jit_prof_threshold=<v>` override is read as a DOUBLE
    /// (`zend_strtod` leading prefix) and REPORTED truncated toward zero, never through the
    /// quantity parser. VERIFIED on real PHP 8.2.31:
    ///
    /// | `-d …=` | `opcache_get_configuration()` | `ini_get` |
    /// |---------|-------------------------------|-----------|
    /// | `2.7`   | `int(2)`                      | `'2.7'`   |
    /// | `0.5`   | `int(0)`                      | `'0.5'`   |
    /// | `-1.9`  | `int(-1)`                     | `'-1.9'`  |
    /// | `0x10`  | `int(0)`                      | `'0x10'`  |
    /// | `abc`   | `int(0)`                      | `'abc'`   |
    ///
    /// The `0x10 → 0` row is the one that PROVES it is not the quantity parser, which reads `0x10`
    /// as 16. On 8.3+ the same overrides yield the untruncated float.
    #[test]
    fn jit_prof_threshold_override_truncates_only_in_the_82_profile() {
        let parse82 = |raw: &str| parse_ini_override(JIT_PROF_THRESHOLD, raw, &base(80200, JIT_PROF_THRESHOLD));
        for (raw, expected) in [("2.7", 2i64), ("0.5", 0), ("-1.9", -1), ("0x10", 0), ("abc", 0)] {
            assert_eq!(parse82(raw), Some(DirectiveValue::Int(expected)), "8.2 {raw}");
        }
        let parse85 = |raw: &str| parse_ini_override(JIT_PROF_THRESHOLD, raw, &base(80500, JIT_PROF_THRESHOLD));
        assert_eq!(parse85("2.7"), Some(DirectiveValue::Float(2.7)));
        assert_eq!(parse85("0x10"), Some(DirectiveValue::Float(0.0)));
    }

    /// Every range-validated integer directive REFUSES a value outside its bounds (leaving the
    /// compiled default) and ACCEPTS both boundary values. The bounds and the reference probes
    /// behind each row are tabulated in [`directive_int_range`]; the three off-by-one rows
    /// (`jit_max_loop_unrolls`, `jit_max_recursive_calls`, `jit_max_recursive_returns`) are the
    /// ones where reference PHP's own warning names a bound it then rejects.
    #[test]
    fn range_validated_directives_refuse_out_of_range_values() {
        let cases: [(&str, u32, i64, i64); 12] = [
            ("opcache.max_accelerated_files", 80500, 200, 1_000_000),
            ("opcache.interned_strings_buffer", 80500, 0, 32_767),
            ("opcache.jit_blacklist_root_trace", 80500, 0, 255),
            ("opcache.jit_blacklist_side_trace", 80500, 0, 255),
            ("opcache.jit_hot_func", 80500, 0, 255),
            ("opcache.jit_hot_loop", 80500, 0, 255),
            ("opcache.jit_hot_return", 80500, 0, 255),
            ("opcache.jit_hot_side_exit", 80500, 0, 255),
            ("opcache.jit_max_loop_unrolls", 80500, 1, 9),
            ("opcache.jit_max_recursive_calls", 80500, 1, 9),
            ("opcache.jit_max_recursive_returns", 80500, 0, 3),
            ("opcache.jit_max_trace_length", 80500, 4, 1024),
        ];
        for (name, version, lo, hi) in cases {
            assert_eq!(directive_int_range(name), Some((lo, hi)), "{name} range");
            let default = base(version, name);
            for accepted in [lo, hi] {
                assert_eq!(
                    parse_ini_override(name, &accepted.to_string(), &default),
                    Some(DirectiveValue::Int(accepted)),
                    "{name} must accept {accepted}"
                );
            }
            for refused in [lo - 1, hi + 1] {
                assert_eq!(
                    parse_ini_override(name, &refused.to_string(), &default),
                    None,
                    "{name} must refuse {refused} and keep the default"
                );
            }
        }
        // Every OTHER integer directive is unbounded (the generic `OnUpdateLong` handler).
        assert_eq!(directive_int_range("opcache.max_file_size"), None);
        assert_eq!(directive_int_range("opcache.revalidate_freq"), None);
        assert_eq!(directive_int_range(MEMORY_CONSUMPTION), None);
    }

    /// `opcache.max_accelerated_files` is `atoi`-read like `opcache.memory_consumption`, NOT
    /// quantity-read: a `K`/`M`/`G` suffix or a `0x` prefix is ignored, so the leading decimal run
    /// alone decides. VERIFIED on reference PHP 8.5.6: `-d opcache.max_accelerated_files=8K` and
    /// `=0x1000` both report the untouched default 10000 (`atoi` reads 8 and 0, each below the 200
    /// floor), where the quantity parser would have stored 8192 and 4096.
    #[test]
    fn max_accelerated_files_uses_atoi_not_the_quantity_parser() {
        let files = base(80500, MAX_ACCELERATED_FILES);
        let parse = |raw: &str| parse_ini_override(MAX_ACCELERATED_FILES, raw, &files);
        assert_eq!(parse("8K"), None, "atoi reads 8, below the 200 floor");
        assert_eq!(parse("0x1000"), None, "atoi reads 0, below the 200 floor");
        // A suffix on an IN-RANGE leading run is simply dropped.
        assert_eq!(parse("5000M"), Some(DirectiveValue::Int(5_000)));
        assert_eq!(parse("300"), Some(DirectiveValue::Int(300)));
    }

    /// The ten JIT range validators emit their VERBATIM reference warning when they refuse a
    /// value, in the two message shapes php-src uses; the two `zend_accel_error` directives and
    /// every unbounded directive emit nothing.
    ///
    /// The exact reference lines (PHP 8.5.6 stderr, minus the `Warning: ` prefix and the
    /// ` in Unknown on line 0` suffix) are tabulated in [`ini_range_warning`]. The ORDER matters
    /// for a value that is both malformed AND out of range: php-src runs
    /// `zend_ini_parse_quantity_warn` first, so `--ini opcache.jit_hot_func=999abc` prints the
    /// quantity line and THEN the range line — VERIFIED byte-for-byte.
    #[test]
    fn jit_range_refusals_carry_their_reference_warning() {
        let warn = |name: &str, raw: &str| {
            ini_override_warnings(80500, &[(name.to_string(), raw.to_string())])
        };
        assert_eq!(
            warn("opcache.jit_hot_func", "256"),
            vec![
                "Invalid \"opcache.jit_hot_func\" setting; using default value instead. \
                 Should be between 0 and 255"
                    .to_string()
            ]
        );
        assert_eq!(
            warn("opcache.jit_max_loop_unrolls", "10"),
            vec!["Invalid \"opcache.jit_max_loop_unrolls\" setting. Should be between 1 and 10".to_string()]
        );
        assert_eq!(
            warn("opcache.jit_max_recursive_calls", "10"),
            vec!["Invalid \"opcache.jit_max_recursive_calls\" setting. Should be between 1 and 10".to_string()]
        );
        assert_eq!(
            warn("opcache.jit_max_recursive_returns", "4"),
            vec!["Invalid \"opcache.jit_max_recursive_returns\" setting. Should be between 0 and 4".to_string()]
        );
        assert_eq!(
            warn("opcache.jit_max_trace_length", "3"),
            vec!["Invalid \"opcache.jit_max_trace_length\" setting. Should be between 4 and 1024".to_string()]
        );
        // Both lines, quantity first, for a value that is malformed AND out of range.
        assert_eq!(
            warn("opcache.jit_hot_func", "999abc"),
            vec![
                "Invalid \"opcache.jit_hot_func\" setting. Invalid quantity \"999abc\": unknown \
                 multiplier \"c\", interpreting as \"999\" for backwards compatibility"
                    .to_string(),
                "Invalid \"opcache.jit_hot_func\" setting; using default value instead. \
                 Should be between 0 and 255"
                    .to_string(),
            ]
        );
        // An IN-RANGE value warns not at all.
        assert!(warn("opcache.jit_hot_func", "255").is_empty());
        // The two `zend_accel_error` directives are silent at the default verbosity, on BOTH
        // channels — VERIFIED that `-d opcache.max_accelerated_files=12abc` prints nothing.
        assert!(warn(MAX_ACCELERATED_FILES, "199").is_empty());
        assert!(warn(MAX_ACCELERATED_FILES, "12abc").is_empty());
        assert!(warn("opcache.interned_strings_buffer", "100000").is_empty());
        // ...but interned_strings_buffer IS quantity-read, so a malformed value still warns.
        assert_eq!(
            warn("opcache.interned_strings_buffer", "12abc"),
            vec![
                "Invalid \"opcache.interned_strings_buffer\" setting. Invalid quantity \"12abc\": \
                 unknown multiplier \"c\", interpreting as \"12\" for backwards compatibility"
                    .to_string()
            ]
        );
        // `opcache.jit_prof_threshold` never reaches the quantity parser at all.
        assert!(warn(JIT_PROF_THRESHOLD, "0.005").is_empty());
        assert!(warn(JIT_PROF_THRESHOLD, "abc").is_empty());
    }

    /// `max_cached_keys` is the first php-src prime `>= opcache.max_accelerated_files`. The full
    /// reference table (PHP 8.5.6) is in [`accel_hash_max_num_entries`]; the `223 → 223` row is
    /// the one that distinguishes `>=` from a strict `>`.
    #[test]
    fn max_cached_keys_rounds_up_through_the_prime_table() {
        for (files, expected) in [
            (200i64, 223i64),
            (201, 223),
            (222, 223),
            (223, 223),
            (224, 463),
            (462, 463),
            (463, 463),
            (464, 983),
            (1_000, 1_979),
            (3_000, 3_907),
            (10_000, 16_229),
            (65_536, 130_987),
            (999_999, 1_048_793),
            (1_000_000, 1_048_793),
        ] {
            assert_eq!(
                accel_hash_max_num_entries(files),
                expected,
                "max_accelerated_files {files}"
            );
        }
        // Above the last prime the size passes through unchanged (unreachable for this directive,
        // whose ceiling is 1000000, but it is php-src's own fall-through).
        assert_eq!(accel_hash_max_num_entries(2_000_000), 2_000_000);
    }

    /// `opcache.file_cache` is the ONE directive `ini_get_all()` reports as PHP `null`.
    ///
    /// VERIFIED on reference PHP 8.5.6 by scanning all 54 `opcache.*` entries for a NULL on either
    /// side — exactly one hit — in BOTH the `$details=true` and `$details=false` surfaces. See
    /// [`directive_ini_null_default`].
    #[test]
    fn file_cache_is_the_only_null_defaulting_directive() {
        for version in [80200u32, 80300, 80400, 80500] {
            let null_names: Vec<&str> = opcache_directives(version)
                .into_iter()
                .map(|(name, _)| name)
                .filter(|name| directive_ini_null_default(name))
                .collect();
            assert_eq!(null_names, vec!["opcache.file_cache"], "for {version}");
        }
        // The RAW `ini_get` string is still the empty string, in every configuration — the NULL is
        // visible through `ini_get_all()` alone (VERIFIED: `ini_get('opcache.file_cache')` is
        // `string(0) ""` whether the directive is unset, set to `''`, or set to a path).
        assert_eq!(ini(80500, "opcache.file_cache"), "");
    }

    /// Resolves a directive's raw INI string for a version by name.
    fn ini(version: u32, key: &str) -> String {
        let directives = opcache_directives(version);
        let (_, value) = directives
            .iter()
            .find(|(name, _)| *name == key)
            .unwrap_or_else(|| panic!("directive {key} must exist for {version}"));
        directive_ini_string(key, value)
    }

    /// The raw INI strings for the 8.5 target match reference PHP 8.5.6 `ini_get`,
    /// including the four non-derivable overrides.
    #[test]
    fn php85_ini_strings_match_reference() {
        // Booleans render "1"/"0" (opcache convention, not session's "1"/"").
        assert_eq!(ini(80500, "opcache.enable"), "1");
        assert_eq!(ini(80500, "opcache.enable_cli"), "0");
        assert_eq!(ini(80500, "opcache.protect_memory"), "0");
        assert_eq!(ini(80500, "opcache.file_cache_read_only"), "0");
        // Overrides: raw MiB / percent / hex / size string, not the normalized forms.
        assert_eq!(ini(80500, "opcache.memory_consumption"), "128");
        assert_eq!(ini(80500, "opcache.interned_strings_buffer"), "8");
        assert_eq!(ini(80500, "opcache.max_accelerated_files"), "10000");
        assert_eq!(ini(80500, "opcache.max_wasted_percentage"), "5");
        assert_eq!(ini(80500, "opcache.optimization_level"), "0x7FFEBFFF");
        assert_eq!(ini(80500, "opcache.jit_buffer_size"), "64M");
        // Floats render their shortest decimal; strings pass through; empty is "".
        assert_eq!(ini(80500, "opcache.jit_prof_threshold"), "0.005");
        assert_eq!(ini(80500, "opcache.jit"), "disable");
        assert_eq!(ini(80500, "opcache.lockfile_path"), "/tmp");
        assert_eq!(ini(80500, "opcache.preload"), "");
        assert_eq!(ini(80500, "opcache.jit_hot_loop"), "61");
    }

    /// The per-version raw-string deltas hold: 8.2/8.3 flip `jit` and `jit_buffer_size`,
    /// and 8.4 keeps `jit_hot_loop` at 64 while 8.5 lowers it to 61.
    #[test]
    fn ini_string_version_deltas_hold() {
        assert_eq!(ini(80200, "opcache.jit"), "tracing");
        assert_eq!(ini(80300, "opcache.jit"), "tracing");
        assert_eq!(ini(80400, "opcache.jit"), "disable");
        assert_eq!(ini(80500, "opcache.jit"), "disable");

        assert_eq!(ini(80200, "opcache.jit_buffer_size"), "0");
        assert_eq!(ini(80300, "opcache.jit_buffer_size"), "0");
        assert_eq!(ini(80400, "opcache.jit_buffer_size"), "64M");
        assert_eq!(ini(80500, "opcache.jit_buffer_size"), "64M");

        assert_eq!(ini(80400, "opcache.jit_hot_loop"), "64");
        assert_eq!(ini(80500, "opcache.jit_hot_loop"), "61");

        // 8.2-only directive carries its raw "0".
        assert_eq!(ini(80200, "opcache.consistency_checks"), "0");
    }

    /// Access levels match reference PHP 8.5.6: the PHP_INI_ALL set is 7, everything else
    /// is PHP_INI_SYSTEM = 4, and the 8.2-only directive is PHP_INI_ALL.
    #[test]
    fn directive_access_levels_match_reference() {
        assert_eq!(directive_access("opcache.enable"), 7);
        assert_eq!(directive_access("opcache.validate_timestamps"), 7);
        assert_eq!(directive_access("opcache.revalidate_freq"), 7);
        assert_eq!(directive_access("opcache.jit"), 7);
        assert_eq!(directive_access("opcache.jit_prof_threshold"), 7);
        assert_eq!(directive_access("opcache.consistency_checks"), 7);
        // PHP_INI_SYSTEM majority.
        assert_eq!(directive_access("opcache.enable_cli"), 4);
        assert_eq!(directive_access("opcache.memory_consumption"), 4);
        assert_eq!(directive_access("opcache.jit_buffer_size"), 4);
        assert_eq!(directive_access("opcache.preload"), 4);
        assert_eq!(directive_access("opcache.file_cache_read_only"), 4);
    }

    /// Resolves a directive's default typed value for a version by name (test helper).
    fn base(version: u32, key: &str) -> DirectiveValue {
        opcache_directives(version)
            .into_iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("directive {key} must exist for {version}"))
    }

    /// A bool override NEVER fails: `zend_ini_parse_bool` answers `true` for `true`/`yes`/`on`
    /// and otherwise for any string with a non-zero `atoi`, and `false` for everything else —
    /// including `garbage`. Reference table verified on PHP 8.5.6 with
    /// `-d opcache.save_comments=<v>`, reading `opcache_get_configuration()['directives']`.
    #[test]
    fn parse_ini_override_bool_forms() {
        let b = base(80500, "opcache.enable_cli");
        for truthy in [
            "1", "on", "On", "ON", "oN", "TRUE", "true", "yes", "Yes", "2", "-1", "007",
        ] {
            assert_eq!(
                parse_ini_override("opcache.enable_cli", truthy, &b),
                Some(DirectiveValue::Bool(true)),
                "{truthy} must be truthy"
            );
        }
        for falsey in [
            "0", "off", "Off", "false", "no", "", "none", "None", "null", "NULL",
        ] {
            assert_eq!(
                parse_ini_override("opcache.enable_cli", falsey, &b),
                Some(DirectiveValue::Bool(false)),
                "{falsey} must be falsey"
            );
        }
        // THE FIX: an unparseable spelling falls to FALSE and is STORED. Reference PHP 8.5.6
        // `-d opcache.save_comments=garbage` reports `bool(false)`, not the compiled default.
        for garbage in ["garbage", "maybe", "yess", "onn", "-", "+"] {
            assert_eq!(
                parse_ini_override("opcache.enable_cli", garbage, &b),
                Some(DirectiveValue::Bool(false)),
                "{garbage} must store false, not fall back to the default"
            );
        }
    }

    /// The INI SCANNER rewrite runs ahead of every type handler, for every directive type, and
    /// the string `ini_get()` reports is the REWRITTEN one. Reference table verified on PHP
    /// 8.5.6 with `-d opcache.preferred_memory_model=<v>` — a plain STRING directive, so no
    /// boolean handler is anywhere on the path.
    #[test]
    fn ini_scanner_rewrites_bool_aliases_for_every_directive() {
        for truthy in ["on", "On", "ON", "oN", "true", "True", "TRUE", "yes", "Yes", "YES"] {
            assert_eq!(ini_scanner_value(truthy), "1", "{truthy} rewrites to \"1\"");
        }
        for falsey in [
            "off", "Off", "OFF", "false", "False", "FALSE", "no", "No", "NO", "none", "None",
            "NONE", "nOnE", "null", "NULL",
        ] {
            assert_eq!(ini_scanner_value(falsey), "", "{falsey} rewrites to \"\"");
        }
        // Surrounding whitespace is part of the alias token (verified with a real `-c` ini file).
        assert_eq!(ini_scanner_value("  on  "), "1");
        assert_eq!(ini_scanner_value("\ttrue\n"), "1");
        // Anything else is passed through byte-for-byte, whitespace included.
        for verbatim in ["1", "0", "12abc", "tracing", "  12  ", "", "onn", "nones"] {
            assert_eq!(
                ini_scanner_value(verbatim),
                verbatim,
                "{verbatim} is not an alias"
            );
        }
        // The rewrite is visible on a STRING directive's reported value, and it does NOT change
        // the jit MODE mapping: `on` becomes `"1"`, which is still a TRACING spelling.
        let model = base(80500, "opcache.preferred_memory_model");
        let overrides = vec![("opcache.preferred_memory_model".to_string(), "on".to_string())];
        assert_eq!(
            effective_directive_ini_string("opcache.preferred_memory_model", &model, &overrides),
            "1"
        );
        let jit_on = vec![("opcache.jit".to_string(), "on".to_string())];
        let jit = base(80500, "opcache.jit");
        assert_eq!(
            effective_directive_ini_string("opcache.jit", &jit, &jit_on),
            "1",
            "ini_get reports the scanner rewrite"
        );
        let config = effective_jit_config(80500, &jit_on);
        assert_eq!(
            (config.kind, config.opt_level, config.opt_flags),
            (5, 4, 6),
            "jit=on still selects TRACING"
        );
    }

    /// The QUANTITY parser reproduces `zend_ini_parse_quantity` exactly — value AND diagnostic.
    /// Every row was captured from reference PHP 8.5.6 via `-d opcache.max_file_size=<v>`,
    /// reading `opcache_get_configuration()['directives']` and the startup warning text.
    #[test]
    fn parse_ini_quantity_matches_reference() {
        let unknown = |raw: &str, multiplier: char, prefix: &str| {
            Some(format!(
                "Invalid quantity \"{raw}\": unknown multiplier \"{multiplier}\", \
                 interpreting as \"{prefix}\" for backwards compatibility"
            ))
        };
        let trailing = |raw: &str, prefix: &str| {
            Some(format!(
                "Invalid quantity \"{raw}\", interpreting as \"{prefix}\" \
                 for backwards compatibility"
            ))
        };
        let no_digits = |raw: &str, why: &str| {
            Some(format!(
                "Invalid quantity \"{raw}\": {why}, interpreting as \"0\" \
                 for backwards compatibility"
            ))
        };
        let range = |raw: &str| {
            Some(format!(
                "Invalid quantity \"{raw}\": value is out of range, \
                 using overflow result for backwards compatibility"
            ))
        };

        // Clean values: no diagnostic.
        for (raw, expected) in [
            ("12", 12_i64),
            ("+12", 12),
            ("-12", -12),
            ("12K", 12_288),
            ("12k", 12_288),
            ("12M", 12_582_912),
            ("12G", 12_884_901_888),
            ("12  M", 12_582_912),
            ("  12  ", 12),
            ("0x10", 16),
            ("0X10", 16),
            ("-0x10", -16),
            ("010", 8),
            ("0777", 511),
            ("0b101", 5),
            ("+0b11", 3),
            ("0x1G", 1_073_741_824),
            ("0b1K", 1_024),
            ("0", 0),
            ("00", 0),
            ("", 0),
            ("  ", 0),
        ] {
            assert_eq!(parse_ini_quantity(raw), (expected, None), "clean {raw:?}");
        }

        // No leading digits: 0 plus a diagnostic. The value is still STORED (reference PHP
        // reports `int(0)`, not the compiled default) — that is fix #3.
        for raw in ["garbage", "K", "x", "-garbage", "+garbage", "0xZZ", "0b2"] {
            assert_eq!(
                parse_ini_quantity(raw),
                (0, no_digits(raw, "no valid leading digits")),
                "no-digits {raw:?}"
            );
        }
        for raw in ["0x", "0X", "0b"] {
            assert_eq!(
                parse_ini_quantity(raw),
                (0, no_digits(raw, "no digits after base prefix")),
                "base-prefix {raw:?}"
            );
        }

        // Unknown multiplier: the LAST character is the multiplier candidate, and the prefix is
        // a SLICE of the original up to the first non-space after the digits.
        assert_eq!(
            parse_ini_quantity("12abc"),
            (12, unknown("12abc", 'c', "12"))
        );
        assert_eq!(parse_ini_quantity("12KB"), (12, unknown("12KB", 'B', "12")));
        assert_eq!(parse_ini_quantity("12.9"), (12, unknown("12.9", '9', "12")));
        assert_eq!(parse_ini_quantity("1e3"), (1, unknown("1e3", '3', "1")));
        assert_eq!(parse_ini_quantity("12,5"), (12, unknown("12,5", '5', "12")));
        assert_eq!(parse_ini_quantity("08"), (0, unknown("08", '8', "0")));
        assert_eq!(parse_ini_quantity("12 x"), (12, unknown("12 x", 'x', "12 ")));
        assert_eq!(
            parse_ini_quantity("12 M x"),
            (12, unknown("12 M x", 'x', "12 ")),
            "the stray trailing char wins over the M: no multiply"
        );
        assert_eq!(parse_ini_quantity("1  2"), (1, unknown("1  2", '2', "1  ")));

        // Trailing data with a VALID last-character multiplier: the multiply still happens, and
        // the prefix appends the multiplier CHARACTER AS WRITTEN (note the lowercase `k`).
        assert_eq!(
            parse_ini_quantity("12MM"),
            (12_582_912, trailing("12MM", "12M"))
        );
        assert_eq!(
            parse_ini_quantity("12M M"),
            (12_582_912, trailing("12M M", "12M"))
        );
        assert_eq!(
            parse_ini_quantity("9M M M"),
            (9_437_184, trailing("9M M M", "9M"))
        );
        assert_eq!(
            parse_ini_quantity("12 x M"),
            (12_582_912, trailing("12 x M", "12 M"))
        );
        assert_eq!(
            parse_ini_quantity("12 K junk"),
            (12_288, trailing("12 K junk", "12 k")),
            "the prefix takes junk's final k, not the K the user typed"
        );

        // Unsigned accumulation: `strtoul` then a cast, not a saturation at i64::MAX.
        assert_eq!(
            parse_ini_quantity("18446744073709551615"),
            (-1, range("18446744073709551615"))
        );
        assert_eq!(
            parse_ini_quantity("18446744073709551616"),
            (-1, range("18446744073709551616"))
        );
        assert_eq!(
            parse_ini_quantity("-99999999999999999999"),
            (-1, range("-99999999999999999999"))
        );
        assert_eq!(
            parse_ini_quantity("9223372036854775808"),
            (i64::MIN, range("9223372036854775808"))
        );
    }

    /// Integer overrides parse decimal, `K`/`M`/`G` byte suffixes, and `0x` hex; a non-numeric
    /// body is rejected.
    #[test]
    fn parse_ini_override_int_forms() {
        let files = base(80500, "opcache.max_accelerated_files");
        assert_eq!(
            parse_ini_override("opcache.max_accelerated_files", "20000", &files),
            Some(DirectiveValue::Int(20_000))
        );
        // K/M/G suffixes multiply by 1024^n (zend_atol) for a plain size directive.
        let jit_buffer = base(80500, "opcache.jit_buffer_size");
        assert_eq!(
            parse_ini_override("opcache.jit_buffer_size", "64M", &jit_buffer),
            Some(DirectiveValue::Int(67_108_864))
        );
        assert_eq!(
            parse_ini_override("opcache.jit_buffer_size", "128", &jit_buffer),
            Some(DirectiveValue::Int(128)),
            "a plain jit_buffer_size integer is bytes, not mebibytes"
        );
        // `0x` hex for optimization_level.
        let opt = base(80500, "opcache.optimization_level");
        assert_eq!(
            parse_ini_override("opcache.optimization_level", "0x7FFEBFFF", &opt),
            Some(DirectiveValue::Int(2_147_401_727))
        );
        // A non-numeric integer override is STORED as 0 by the generic quantity handler, not
        // ignored. Reference PHP 8.5.6 `-d opcache.interned_strings_buffer=garbage` reports
        // `int(0)` with a warning.
        assert_eq!(
            parse_ini_override(
                "opcache.interned_strings_buffer",
                "garbage",
                &base(80500, "opcache.interned_strings_buffer")
            ),
            Some(DirectiveValue::Int(0))
        );
        // `opcache.max_accelerated_files` does NOT behave that way: it is RANGE-VALIDATED with a
        // floor of 200, so the `atoi` reading of 0 is REFUSED and the compiled default stands.
        // VERIFIED on reference PHP 8.5.6: `-d opcache.max_accelerated_files=199` and `=1000001`
        // both report 10000. See `directive_int_range`.
        assert_eq!(
            parse_ini_override("opcache.max_accelerated_files", "lots", &files),
            None
        );
        // ...and a value with a leading numeric prefix keeps that prefix.
        assert_eq!(
            parse_ini_override("opcache.max_file_size", "12abc", &base(80500, "opcache.max_file_size")),
            Some(DirectiveValue::Int(12))
        );
        // The scanner rewrite reaches the quantity parser: `on` arrives as `"1"`, `none` as `""`.
        assert_eq!(
            parse_ini_override("opcache.max_file_size", "on", &base(80500, "opcache.max_file_size")),
            Some(DirectiveValue::Int(1))
        );
        assert_eq!(
            parse_ini_override("opcache.max_file_size", "none", &base(80500, "opcache.max_file_size")),
            Some(DirectiveValue::Int(0))
        );
    }

    /// `opcache.memory_consumption` is the ONE integer directive php-src does not route through
    /// the quantity parser: its handler reads the raw string with `atoi`, treats the result as a
    /// MEBIBYTE count, and REFUSES the store below the 8 MiB floor. Every row verified on
    /// reference PHP 8.5.6 (`-d opcache.memory_consumption=<v>`).
    #[test]
    fn parse_ini_override_memory_consumption_mib_semantics() {
        let mem = base(80500, "opcache.memory_consumption");
        let parse = |raw: &str| parse_ini_override("opcache.memory_consumption", raw, &mem);
        assert_eq!(
            parse("256"),
            Some(DirectiveValue::Int(268_435_456)),
            "plain 256 is 256 MiB"
        );
        assert_eq!(
            parse("256M"),
            Some(DirectiveValue::Int(268_435_456)),
            "atoi stops at the M, so 256M is the same 256 MiB"
        );
        assert_eq!(
            parse("256K"),
            Some(DirectiveValue::Int(268_435_456)),
            "the suffix is IGNORED, not read as a byte size: reference reports 256 MiB"
        );
        assert_eq!(
            parse("8"),
            Some(DirectiveValue::Int(8 * 1024 * 1024)),
            "8 MiB is exactly the floor and is accepted"
        );
        // Below the floor → the store is REFUSED and the compiled default survives. `1G` is the
        // trap: atoi reads it as 1, not as a gibibyte.
        for refused in ["1G", "4", "0", "garbage", "-16"] {
            assert_eq!(parse(refused), None, "{refused} is below the 8 MiB floor");
        }
    }

    /// `opcache.max_wasted_percentage` is a PERCENT: reference PHP accepts `1..=50` and stores
    /// `percent / 100.0`, so `"10"` normalizes to `0.1` and the default raw `"5"` to `0.05`.
    /// Out-of-range values (including `0.1`, which is BELOW 1 — reference PHP rejects it) and
    /// non-numeric bodies are rejected so the compiled-in default survives. A string directive
    /// takes the raw override verbatim.
    #[test]
    fn parse_ini_override_float_and_string() {
        let pct = base(80500, "opcache.max_wasted_percentage");
        // In range: divided by 100 into the normalized fraction.
        assert_eq!(
            parse_ini_override("opcache.max_wasted_percentage", "10", &pct),
            Some(DirectiveValue::Float(0.1)),
            "a 10% override normalizes to 0.1, not 10.0"
        );
        assert_eq!(
            parse_ini_override("opcache.max_wasted_percentage", "1", &pct),
            Some(DirectiveValue::Float(0.01)),
            "1 is the inclusive lower bound"
        );
        assert_eq!(
            parse_ini_override("opcache.max_wasted_percentage", "50", &pct),
            Some(DirectiveValue::Float(0.5)),
            "50 is the inclusive upper bound"
        );
        // Out of range: rejected, the default (0.05) is kept by the caller.
        assert_eq!(
            parse_ini_override("opcache.max_wasted_percentage", "0.1", &pct),
            None,
            "0.1 is below the 1..=50 percent range; reference PHP rejects it"
        );
        assert_eq!(
            parse_ini_override("opcache.max_wasted_percentage", "0", &pct),
            None
        );
        assert_eq!(
            parse_ini_override("opcache.max_wasted_percentage", "60", &pct),
            None,
            "60 is above the 50% ceiling"
        );
        assert_eq!(
            parse_ini_override("opcache.max_wasted_percentage", "-5", &pct),
            None
        );
        assert_eq!(
            parse_ini_override("opcache.max_wasted_percentage", "nope", &pct),
            None
        );
        // The other float directive keeps the plain-f64 path (no percent scaling).
        let jit_threshold = base(80500, "opcache.jit_prof_threshold");
        assert_eq!(
            parse_ini_override("opcache.jit_prof_threshold", "0.1", &jit_threshold),
            Some(DirectiveValue::Float(0.1)),
            "jit_prof_threshold is a plain float, not a percent"
        );
        let jit = base(80500, "opcache.jit");
        assert_eq!(
            parse_ini_override("opcache.jit", "tracing", &jit),
            Some(DirectiveValue::Str("tracing"))
        );
    }

    /// `opcache.max_wasted_percentage` reads its percent with C `atoi`, NOT a float parse.
    ///
    /// Every row is byte-verified against reference PHP 8.5.6 (`-d opcache.max_wasted_percentage=<v>`,
    /// reading `opcache_get_configuration()['directives']`). The fractional and exponent rows are
    /// the ones that discriminate: a float parse would give `2.5 → 0.025` and `3e1 → 0.3`, and an
    /// earlier revision of this parser did exactly that.
    #[test]
    fn max_wasted_percentage_uses_atoi_truncation() {
        let pct = base(80500, "opcache.max_wasted_percentage");
        let parse = |raw: &str| parse_ini_override("opcache.max_wasted_percentage", raw, &pct);
        // Truncating reads (verified: 2.5 → 0.02, 1.9 → 0.01, 50.9 → 0.5, 3e1 → 0.03).
        assert_eq!(parse("2.5"), Some(DirectiveValue::Float(0.02)));
        assert_eq!(parse("1.9"), Some(DirectiveValue::Float(0.01)));
        assert_eq!(parse("50.9"), Some(DirectiveValue::Float(0.5)));
        assert_eq!(parse("3e1"), Some(DirectiveValue::Float(0.03)));
        // Leading digits win, trailing junk is ignored (verified: 2abc → 0.02).
        assert_eq!(parse("2abc"), Some(DirectiveValue::Float(0.02)));
        // Sign and surrounding whitespace are `atoi`'s own (verified: +3 → 0.03, ' 7 ' → 0.07).
        assert_eq!(parse("+3"), Some(DirectiveValue::Float(0.03)));
        assert_eq!(parse(" 7 "), Some(DirectiveValue::Float(0.07)));
        // `atoi` reads no hex, so 0x10 truncates to 0 and is out of range (verified: default kept).
        assert_eq!(parse("0x10"), None);
    }

    /// A plain float directive is read with `zend_strtod` LEADING-PREFIX semantics and therefore
    /// never fails: garbage yields `0.0` rather than keeping the compiled default.
    ///
    /// Byte-verified against reference PHP 8.5.6 (`-d opcache.jit_prof_threshold=<v>`): `0.5` →
    /// `0.5`, `1e-3` → `0.001`, `3` → `3.0`, `-1` → `-1.0`, `0.005x` → `0.005`, `abc` → `0.0`,
    /// empty → `0.0`; and `ini_get` reports the raw string verbatim in every one of those cases.
    #[test]
    fn plain_float_uses_strtod_prefix() {
        let threshold = base(80500, "opcache.jit_prof_threshold");
        let parse = |raw: &str| parse_ini_override("opcache.jit_prof_threshold", raw, &threshold);
        assert_eq!(parse("0.5"), Some(DirectiveValue::Float(0.5)));
        assert_eq!(parse("1e-3"), Some(DirectiveValue::Float(0.001)));
        assert_eq!(parse("3"), Some(DirectiveValue::Float(3.0)));
        assert_eq!(parse("-1"), Some(DirectiveValue::Float(-1.0)));
        // PREFIX, not a whole-string parse: the trailing `x` is dropped, the value still stores.
        assert_eq!(parse("0.005x"), Some(DirectiveValue::Float(0.005)));
        // No valid prefix ⇒ 0.0, and the store still SUCCEEDS (so `ini_get` reports the raw).
        assert_eq!(parse("abc"), Some(DirectiveValue::Float(0.0)));
        assert_eq!(parse(""), Some(DirectiveValue::Float(0.0)));
        assert_eq!(parse("   "), Some(DirectiveValue::Float(0.0)));
        // A bare `.` and a bare exponent carry no mantissa digit / no exponent digit.
        assert_eq!(parse("."), Some(DirectiveValue::Float(0.0)));
        assert_eq!(parse("1e"), Some(DirectiveValue::Float(1.0)));
        assert_eq!(parse("1e+"), Some(DirectiveValue::Float(1.0)));
        assert_eq!(parse(".5"), Some(DirectiveValue::Float(0.5)));
        // Because the store always succeeds, `ini_get` reports the raw string verbatim.
        let overrides = vec![("opcache.jit_prof_threshold".to_string(), "abc".to_string())];
        assert_eq!(
            effective_directive_ini_string("opcache.jit_prof_threshold", &threshold, &overrides),
            "abc"
        );
    }

    /// The two environment-variable spellings for a directive: `.` → `__` for the primary (shells
    /// reject a dot in an assignment word) and the literal dotted form as the secondary. The
    /// directive part stays VERBATIM lowercase in both.
    #[test]
    fn env_var_names_have_both_spellings() {
        assert_eq!(
            directive_env_var_names("opcache.jit"),
            (
                "ELEPHC_INI_opcache__jit".to_string(),
                "ELEPHC_INI_opcache.jit".to_string()
            )
        );
        assert_eq!(
            directive_env_var_names("opcache.save_comments"),
            (
                "ELEPHC_INI_opcache__save_comments".to_string(),
                "ELEPHC_INI_opcache.save_comments".to_string()
            )
        );
        // EVERY dot is replaced, so a future multi-dot directive family stays unambiguous.
        assert_eq!(
            directive_env_var_names("session.upload_progress.min_freq").0,
            "ELEPHC_INI_session__upload_progress__min_freq"
        );
        // Both spellings carry the shared prefix and never upper-case the directive part.
        for (name, _) in opcache_directives(80500) {
            let (under, dotted) = directive_env_var_names(name);
            assert!(under.starts_with(RUNTIME_INI_ENV_PREFIX), "{under}");
            assert!(!under.contains('.'), "{under} must be shell-assignable");
            assert_eq!(dotted, format!("{RUNTIME_INI_ENV_PREFIX}{name}"));
            assert_eq!(under, dotted.replace('.', "__"));
        }
    }

    /// The runtime-override scope rule, asserted over the WHOLE matrix of every maintained
    /// version: exactly the ten directives elephc derives compiled-in behavior from are excluded,
    /// and every other directive is overridable.
    #[test]
    fn runtime_override_scope_covers_every_directive() {
        /// The directives whose value is consumed at COMPILE TIME to bake code or constants.
        const EXCLUDED: [&str; 10] = [
            "opcache.enable",
            "opcache.enable_cli",
            "opcache.memory_consumption",
            "opcache.interned_strings_buffer",
            "opcache.max_accelerated_files",
            "opcache.revalidate_freq",
            "opcache.jit",
            "opcache.jit_buffer_size",
            "opcache.restrict_api",
            "opcache.preload",
        ];
        for version in [80200u32, 80300, 80400, 80500] {
            let directives = opcache_directives(version);
            for (name, _) in &directives {
                assert_eq!(
                    directive_runtime_overridable(name),
                    !EXCLUDED.contains(name),
                    "{name} runtime-override scope disagrees with the excluded set ({version})"
                );
            }
            // Every excluded name is a real directive of this version (no dead exclusions), with
            // the two 8.5-era JIT names checked against the version that actually registers them.
            for excluded in EXCLUDED {
                assert!(
                    directives.iter().any(|(name, _)| *name == excluded),
                    "{excluded} must exist in the {version} table"
                );
            }
            // The overridable majority is the whole rest of the table.
            let overridable = directives
                .iter()
                .filter(|(name, _)| directive_runtime_overridable(name))
                .count();
            assert_eq!(overridable, directives.len() - EXCLUDED.len());
        }
        // 8.5 registers 54 directives, so 44 are runtime-overridable.
        assert_eq!(opcache_directives(80500).len(), 54);
        assert_eq!(
            opcache_directives(80500)
                .iter()
                .filter(|(name, _)| directive_runtime_overridable(name))
                .count(),
            44
        );
    }

    /// The PHP-side type code mirrors `parse_ini_override`'s Rust type dispatch for every
    /// directive, with the `max_wasted_percentage` percent code keyed AHEAD of the generic float
    /// exactly as the match arms are.
    #[test]
    fn env_type_codes_mirror_the_rust_dispatch() {
        for version in [80200u32, 80300, 80400, 80500] {
            for (name, value) in opcache_directives(version) {
                let code = directive_env_type_code(name, &value);
                let expected = match value {
                    // `opcache.jit_prof_threshold` is name-keyed AHEAD of the type match in
                    // `directive_env_type_code`, exactly as it is in `parse_ini_override`: it is a
                    // `double` READ on every version, and the 8.2 profile merely REPORTS it
                    // truncated to an int, which is the `'t'` code. See `JIT_PROF_THRESHOLD`.
                    _ if name == JIT_PROF_THRESHOLD => match value {
                        DirectiveValue::Int(_) => 't',
                        _ => 'f',
                    },
                    DirectiveValue::Bool(_) => 'b',
                    DirectiveValue::Int(_) => 'i',
                    DirectiveValue::Float(_) if name == MAX_WASTED_PERCENTAGE => 'p',
                    DirectiveValue::Float(_) => 'f',
                    DirectiveValue::Str(_) => 's',
                };
                assert_eq!(code, expected, "{name} type code ({version})");
            }
        }
        assert_eq!(
            directive_env_type_code(
                "opcache.max_wasted_percentage",
                &base(80500, "opcache.max_wasted_percentage")
            ),
            'p'
        );
        assert_eq!(
            directive_env_type_code(
                "opcache.jit_prof_threshold",
                &base(80500, "opcache.jit_prof_threshold")
            ),
            'f'
        );
        assert_eq!(
            directive_env_type_code("opcache.save_comments", &base(80500, "opcache.save_comments")),
            'b'
        );
        assert_eq!(
            directive_env_type_code("opcache.max_file_size", &base(80500, "opcache.max_file_size")),
            'i'
        );
        assert_eq!(
            directive_env_type_code("opcache.error_log", &base(80500, "opcache.error_log")),
            's'
        );
    }

    /// `effective_opcache_directives` applies valid overrides, ignores unknown directive names
    /// and unparseable values, and is last-wins for a repeated key. With no overrides it equals
    /// the default table exactly.
    #[test]
    fn effective_directives_apply_and_ignore() {
        // No overrides → identical to the default table.
        assert_eq!(
            effective_opcache_directives(80500, &[]),
            opcache_directives(80500)
        );

        let value = |directives: &[(&'static str, DirectiveValue)], key: &str| {
            directives
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, v)| *v)
        };

        let overrides = vec![
            ("opcache.enable_cli".to_string(), "1".to_string()),
            ("opcache.memory_consumption".to_string(), "256".to_string()),
            // Unknown directive name → ignored.
            ("opcache.not_a_directive".to_string(), "x".to_string()),
            // Malformed int → the quantity parser stores its leading prefix (here: none, so 0)
            // and WARNS; reference PHP does NOT keep the default. See `ini_override_warnings`.
            ("opcache.max_accelerated_files".to_string(), "??".to_string()),
            // Last-wins for a repeated key.
            ("opcache.jit".to_string(), "function".to_string()),
            ("opcache.jit".to_string(), "tracing".to_string()),
        ];
        let effective = effective_opcache_directives(80500, &overrides);
        assert_eq!(
            value(&effective, "opcache.enable_cli"),
            Some(DirectiveValue::Bool(true))
        );
        assert_eq!(
            value(&effective, "opcache.memory_consumption"),
            Some(DirectiveValue::Int(268_435_456))
        );
        // DEFAULTED, not stored: `opcache.max_accelerated_files` is `atoi`-read and
        // RANGE-VALIDATED (200..=1000000), so the 0 that `??` reads is REFUSED and the compiled
        // default stands. VERIFIED on reference PHP 8.5.6 (`-d opcache.max_accelerated_files=??`
        // reports 10000). It also emits NO diagnostic: it never reaches
        // `zend_ini_parse_quantity_warn` (its handler is `atoi`), and its range refusal goes
        // through the verbosity-gated `zend_accel_error` channel — VERIFIED that
        // `-d opcache.max_accelerated_files=12abc` prints nothing at the default
        // `opcache.log_verbosity_level`, while `-d opcache.max_file_size=12abc` does print.
        assert_eq!(
            value(&effective, "opcache.max_accelerated_files"),
            Some(DirectiveValue::Int(10_000))
        );
        assert_eq!(
            ini_override_warnings(80500, &overrides),
            Vec::<String>::new(),
            "an atoi-read, range-validated directive warns on neither channel"
        );
        // Last-wins: tracing, not function.
        assert_eq!(
            value(&effective, "opcache.jit"),
            Some(DirectiveValue::Str("tracing"))
        );
        // Unknown name never appears.
        assert!(value(&effective, "opcache.not_a_directive").is_none());
    }

    /// Resolves a spelling to the `(kind, opt_level, opt_flags)` triple, or `None` when the
    /// spelling is invalid (test helper for the reference mapping tables below).
    fn jit(spelling: &str) -> Option<(i64, i64, i64)> {
        parse_jit_mode(spelling).map(|c| (c.kind, c.opt_level, c.opt_flags))
    }

    /// The KEYWORD spellings map to the reference triples, case-insensitively, including the
    /// INI-scanner-rewritten `"0"`/`"1"` forms. Pinned to reference PHP 8.5.6 AND 8.2.31, which
    /// agree on every row.
    #[test]
    fn jit_keyword_spellings_match_reference() {
        // `disable` and the switched-off family leave every tuning field at zero.
        for off in ["disable", "DISABLE", "Disable", "off", "OFF", "no", "false", "0", "", "0000"] {
            assert_eq!(jit(off), Some((0, 0, 0)), "{off:?} must report the zero triple");
        }
        // `disable` is the only spelling that reports the JIT as NOT configured.
        assert_eq!(parse_jit_mode("disable").map(|c| (c.enabled, c.on)), Some((false, false)));
        assert_eq!(parse_jit_mode("off").map(|c| (c.enabled, c.on)), Some((true, false)));
        assert_eq!(parse_jit_mode("0").map(|c| (c.enabled, c.on)), Some((true, false)));

        // The tracing family — all aliases of the CRTO form 1254.
        for tracing in ["tracing", "TRACING", "Tracing", "on", "ON", "yes", "true", "1"] {
            assert_eq!(
                jit(tracing),
                Some((5, 4, 6)),
                "{tracing:?} must alias tracing (1254)"
            );
        }
        assert_eq!(jit("1254"), jit("tracing"), "tracing IS 1254");

        // The function JIT — the alias of the CRTO form 1205.
        for function in ["function", "FUNCTION", "Function"] {
            assert_eq!(jit(function), Some((0, 5, 6)));
        }
        assert_eq!(jit("1205"), jit("function"), "function IS 1205");

        // Every keyword arm reports the JIT as ON.
        assert_eq!(parse_jit_mode("tracing").map(|c| (c.enabled, c.on)), Some((true, true)));
        assert_eq!(parse_jit_mode("function").map(|c| (c.enabled, c.on)), Some((true, true)));
    }

    /// The CRTO digits decode in the verified order and composition: `T` → `kind`, `O` →
    /// `opt_level`, and `opt_flags` = the `R` register-allocation value OR'd with 4 when `C` is 1.
    #[test]
    fn jit_crto_digits_decode_per_reference() {
        // T (tens) is reported verbatim as `kind`; 4 is the one hole (see the invalid test).
        assert_eq!(jit("1204"), Some((0, 4, 6)));
        assert_eq!(jit("1214"), Some((1, 4, 6)));
        assert_eq!(jit("1224"), Some((2, 4, 6)));
        assert_eq!(jit("1234"), Some((3, 4, 6)));
        assert_eq!(jit("1254"), Some((5, 4, 6)));
        // O (units) is reported verbatim as `opt_level`.
        assert_eq!(jit("1251"), Some((5, 1, 6)));
        assert_eq!(jit("1255"), Some((5, 5, 6)));
        // R (hundreds) ASSIGNS the register-allocation bits: 0 → 0, 1 → LOCAL(1), 2 → GLOBAL(2).
        assert_eq!(jit("1054"), Some((5, 4, 4)), "R=0 leaves only the C bit");
        assert_eq!(jit("1154"), Some((5, 4, 5)), "R=1 is LOCAL(1) | CPU(4)");
        assert_eq!(jit("1254"), Some((5, 4, 6)), "R=2 is GLOBAL(2) | CPU(4)");
        // C (thousands) ORs in CPU_REG_ALLOC(4).
        assert_eq!(jit("0254"), Some((5, 4, 2)), "C=0 drops the CPU bit");
        assert_eq!(jit("0054"), Some((5, 4, 0)), "C=0, R=0 leaves no flags");
        // Shorter spellings are the same number zero-padded, and `+`/leading zeros parse.
        assert_eq!(jit("254"), jit("0254"));
        assert_eq!(jit("54"), jit("0054"));
        assert_eq!(jit("4"), Some((0, 4, 0)));
        assert_eq!(jit("01254"), Some((5, 4, 6)));
        assert_eq!(jit("+1254"), Some((5, 4, 6)));
    }

    /// INVALID spellings are rejected, per digit and per malformed body — each row pinned to a
    /// reference `Warning: Invalid "opcache.jit" setting.` observation.
    #[test]
    fn jit_invalid_spellings_are_rejected() {
        for invalid in [
            "garbage", // non-numeric, non-keyword
            "-1",      // negative
            "1254 ",   // trailing whitespace (the whole-string parse check)
            "1256",    // O = 6 (above ZEND_JIT_LEVEL_OPT_SCRIPT)
            "9",       // O = 9
            "0006",    // O = 6, zero-padded
            "1240",    // O = 0 on a non-zero number
            "0010",    // O = 0
            "9999",    // O = 9
            "1244",    // T = 4, the retired doc-comment trigger
            "1264",    // T = 6
            "1354",    // R = 3
            "1554",    // R = 5
            "2254",    // C = 2
            "5254",    // C = 5
            "12540",   // five digits ⇒ C = 12
        ] {
            assert_eq!(parse_jit_mode(invalid), None, "{invalid:?} must be rejected");
        }
        // T = 4 is a HOLE, not a ceiling: 3 and 5 both remain valid around it.
        assert!(parse_jit_mode("1234").is_some());
        assert!(parse_jit_mode("1254").is_some());
    }

    /// A rejected numeric spelling leaves the verified PARTIAL residue in the engine state:
    /// each digit is assigned before the next is validated. Exercised through
    /// `effective_jit_config` on an 8.5 target, whose `disable` default does not overwrite it.
    #[test]
    fn jit_invalid_numeric_leaves_reference_residue() {
        let residue = |raw: &str| {
            let overrides = vec![("opcache.jit".to_string(), raw.to_string())];
            let c = effective_jit_config(80500, &overrides);
            (c.kind, c.opt_level, c.opt_flags)
        };
        // O rejected first ⇒ nothing assigned at all.
        assert_eq!(residue("1256"), (0, 0, 0));
        assert_eq!(residue("garbage"), (0, 0, 0));
        // O assigned, T rejected.
        assert_eq!(residue("1244"), (0, 4, 0));
        assert_eq!(residue("1345"), (0, 5, 0));
        // O and T assigned, R rejected.
        assert_eq!(residue("1354"), (5, 4, 0));
        assert_eq!(residue("1355"), (5, 5, 0));
        // O, T and R assigned, C rejected.
        assert_eq!(residue("2254"), (5, 4, 2));
        assert_eq!(residue("5254"), (5, 4, 2));
        // The rejection also leaves the JIT reported as NOT configured (the `disable` default
        // is re-applied on top).
        let overrides = vec![("opcache.jit".to_string(), "2254".to_string())];
        let config = effective_jit_config(80500, &overrides);
        assert!(!config.enabled && !config.on);
    }

    /// `effective_jit_config` reproduces the INI two-pass per version: the compiled default is
    /// applied when there is no override AND re-applied on top of a rejected one. On 8.2/8.3 the
    /// `tracing` default OVERWRITES the residue; on 8.4/8.5 the `disable` default preserves it.
    #[test]
    fn jit_effective_config_applies_version_default() {
        let triple = |version: u32, overrides: &[(String, String)]| {
            let c = effective_jit_config(version, overrides);
            (c.kind, c.opt_level, c.opt_flags)
        };
        // No override: the compiled default. 8.2/8.3 default to `tracing`, 8.4/8.5 to `disable`.
        assert_eq!(triple(80200, &[]), (5, 4, 6));
        assert_eq!(triple(80300, &[]), (5, 4, 6));
        assert_eq!(triple(80400, &[]), (0, 0, 0));
        assert_eq!(triple(80500, &[]), (0, 0, 0));

        // A rejected override on 8.2 is fully masked by re-applying `tracing`; the SAME override
        // on 8.5 shows its residue. Verified on php8.2.31 vs php8.5.6 with `-d opcache.jit=1355`.
        let bad = vec![("opcache.jit".to_string(), "1355".to_string())];
        assert_eq!(triple(80200, &bad), (5, 4, 6), "tracing overwrites the residue");
        assert_eq!(triple(80500, &bad), (5, 5, 0), "disable preserves the residue");

        // A valid override wins outright on every version.
        let good = vec![("opcache.jit".to_string(), "function".to_string())];
        assert_eq!(triple(80200, &good), (0, 5, 6));
        assert_eq!(triple(80500, &good), (0, 5, 6));
        // Last-wins for a repeated key, matching a later `-d` on a PHP command line.
        let repeated = vec![
            ("opcache.jit".to_string(), "function".to_string()),
            ("opcache.jit".to_string(), "tracing".to_string()),
        ];
        assert_eq!(triple(80500, &repeated), (5, 4, 6));
    }

    /// An INVALID `opcache.jit` override is not stored: `opcache_get_configuration()` and
    /// `ini_get('opcache.jit')` both keep reporting the compiled default, per version. A VALID
    /// one is stored verbatim.
    #[test]
    fn jit_invalid_override_is_not_stored() {
        let base85 = base(80500, "opcache.jit");
        assert_eq!(parse_ini_override("opcache.jit", "garbage", &base85), None);
        assert_eq!(parse_ini_override("opcache.jit", "1244", &base85), None);
        assert_eq!(
            parse_ini_override("opcache.jit", "1254", &base85),
            Some(DirectiveValue::Str("1254"))
        );

        let bad = vec![("opcache.jit".to_string(), "garbage".to_string())];
        // The reported directive value falls back to the compiled default…
        let value = |version: u32, overrides: &[(String, String)]| {
            effective_opcache_directives(version, overrides)
                .into_iter()
                .find(|(name, _)| *name == "opcache.jit")
                .map(|(_, v)| v)
        };
        assert_eq!(value(80500, &bad), Some(DirectiveValue::Str("disable")));
        assert_eq!(value(80200, &bad), Some(DirectiveValue::Str("tracing")));
        // …and so does the raw INI string.
        assert_eq!(
            effective_directive_ini_string("opcache.jit", &base85, &bad),
            "disable"
        );
        let base82 = base(80200, "opcache.jit");
        assert_eq!(
            effective_directive_ini_string("opcache.jit", &base82, &bad),
            "tracing"
        );
        // A valid override is reported verbatim.
        let good = vec![("opcache.jit".to_string(), "1254".to_string())];
        assert_eq!(
            effective_directive_ini_string("opcache.jit", &base85, &good),
            "1254"
        );
    }

    /// `effective_directive_ini_string` returns the user's raw override verbatim (not a
    /// re-projection), falls back to the default for an unparseable override, and equals the
    /// default projection when unset.
    #[test]
    fn effective_ini_string_returns_raw_override() {
        let mem = base(80500, "opcache.memory_consumption");
        // Unset → default projection ("128").
        assert_eq!(
            effective_directive_ini_string("opcache.memory_consumption", &mem, &[]),
            "128"
        );
        // Overridden → the raw user string verbatim, not the byte count or a normalized form.
        let overrides = vec![("opcache.memory_consumption".to_string(), "256".to_string())];
        assert_eq!(
            effective_directive_ini_string("opcache.memory_consumption", &mem, &overrides),
            "256"
        );
        let overrides_suffix =
            vec![("opcache.memory_consumption".to_string(), "256M".to_string())];
        assert_eq!(
            effective_directive_ini_string("opcache.memory_consumption", &mem, &overrides_suffix),
            "256M"
        );
        // Unparseable override → default projection retained.
        let bad = vec![("opcache.memory_consumption".to_string(), "??".to_string())];
        assert_eq!(
            effective_directive_ini_string("opcache.memory_consumption", &mem, &bad),
            "128"
        );
    }
}
