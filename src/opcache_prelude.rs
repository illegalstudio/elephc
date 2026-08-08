//! Purpose:
//! Injects the OPcache standard-library functions written in elephc-PHP:
//! `opcache_get_configuration()` (returns the compile-time configuration array
//! `['directives' => [...], 'version' => [...], 'blacklist' => []]`),
//! `opcache_reset()` (returns the compile-time cache-enabled boolean), and
//! `opcache_get_status()` (returns `false` when the cache is disabled, or the runtime
//! status array when enabled). All are baked from the version-keyed matrix in
//! `crate::opcache` into declarations built by `build`, whose shapes those baked values are
//! spliced into as typed AST rather than as text.
//!
//! Called from:
//! - `crate::pipeline::compile()` via `inject_if_used`, after include/PDO/tz/list-id
//!   injection and before name resolution, so a user `opcache_get_configuration()` /
//!   `opcache_reset()` call resolves to the injected function through the normal
//!   pipeline (function declaration + literal) with no dedicated codegen or runtime
//!   helper.
//! - `crate::pipeline::compile()` again via `bake_manifest`, immediately AFTER
//!   `autoload::run`, which REBUILDS the manifest-dependent functions against the
//!   COMPLETE script manifest. Injection and manifest baking are split because the
//!   autoloaded file set does not exist until after name resolution, while the
//!   declarations must exist BEFORE it — see `bake_manifest` for the full argument.
//!
//! Key details:
//! - Modeled on `list_id_prelude`/`tz_prelude`: the compiler bakes a configuration array
//!   literal from `opcache_directives(version_id)` and a cache-enabled boolean from
//!   `opcache_cache_enabled(...)`, passes them to the matching `build::*_decl`, and lets the
//!   ordinary literal lowering do the rest — no new `RuntimeFnId`. Neither function is a checker catalog builtin: registering one
//!   would trip the "Cannot redeclare built-in function" guard against this prelude
//!   declaration. Being a real declared function is exactly what makes
//!   `function_exists('opcache_reset')` report `true` (see
//!   `codegen::lower_inst::builtins::lower_function_exists`).
//! - Pay-for-use *per function*: each function is injected only when `detect` finds a
//!   call or a matching string literal (covering `function_exists`/callable forms),
//!   and never when the program already declares its own function of that name (so a
//!   user definition wins and there is no redeclaration conflict). A program that uses
//!   only `opcache_reset` therefore injects only `opcache_reset`.
//! - `opcache_get_configuration` is version-dependent: the compile target's
//!   `PhpVersion` selects the directive set and reported version string.
//! - `opcache_reset` is SAPI-dependent: its baked boolean is the compile-time
//!   cache-enabled state — `false` for a default CLI binary (`opcache.enable_cli`
//!   default), `true` for a `--web` binary (`opcache.enable` default),
//!   matching reference PHP where `php script.php` reports the cache disabled.
//! - `opcache_get_status()['opcache_statistics']['start_time']` is MEMOIZED in a function
//!   `static`, not re-read per call: reference PHP reports the moment the cache started, a fixed
//!   point identical on every call for the life of the process (VERIFIED on 8.5.6 with two calls
//!   two seconds apart). See `get_status_declaration`.
//! - `opcache_get_status()['jit']` reports the FULL reference `opcache.jit` mapping for
//!   `kind`/`opt_level`/`opt_flags` (parsed by `crate::opcache::directives`) under one
//!   explicit clamp — `enabled`/`on` false and both buffer figures 0, always — because an
//!   AOT binary is permanently in reference PHP's own "JIT configured but unavailable in
//!   this process" state. See `render_jit_status` for the clamp's reference evidence.
//! - `opcache.restrict_api` is resolved AT COMPILE TIME, not at runtime. Reference PHP's
//!   guard compares the directive against the ENTRY SCRIPT path
//!   (`SG(request_info).path_translated`), and elephc's entry script is a compile-time
//!   constant while `--ini` is a compile-time flag — so `restrict_api_denies` decides the
//!   outcome once and `inject_if_used` bakes either the normal body or the restricted body
//!   (warning + `false`). This is EXACT, not an approximation: there is no runtime input the
//!   decision could depend on. See `restrict_api_denies` for the verified matching rule and
//!   `RESTRICT_API_WARNING_TEXT` for the verbatim message.
//! - RUNTIME `ELEPHC_INI_*` OVERRIDES are the one part of the INI surface that is NOT frozen at
//!   compile time. `env_override_declarations` bakes a small PHP block that reads
//!   `ELEPHC_INI_opcache__<directive>` (and the dotted `ELEPHC_INI_opcache.<directive>` as a
//!   fallback) through the ordinary `getenv` builtin, normalizes it with the PHP mirror of
//!   `ini_scanner_value` + `parse_ini_override` (`__elephc_ini_scan` then the per-type
//!   normalizer, in that order — the two implementations of every rule must answer identically,
//!   which `tests/opcache_ini_tests.rs::rust_and_php_override_paths_agree` pins by driving the
//!   same value down both paths), and feeds BOTH the typed `opcache_get_configuration()['directives']`
//!   entry and the raw `ini_get()` string — so the two move together exactly as `-d` moves both
//!   in reference PHP. Precedence is baked default → `--ini` → env. This is an elephc EXTENSION
//!   (reference PHP has no per-directive environment override, VERIFIED on 8.5.6), and it is
//!   deliberately NARROWER than `--ini`: only directives elephc merely REPORTS are overridable at
//!   runtime, because every directive it DERIVES compiled-in behavior from would otherwise make
//!   the binary contradict itself. See `crate::opcache::directives::directive_runtime_overridable`
//!   for the excluded set and the argument, and `env_override_declarations` for the injection
//!   rules.
//! - `opcache.preload` is likewise resolved AT COMPILE TIME. Reference PHP preloads during
//!   startup, BEFORE the script runs, and a preload failure is a startup FATAL — so the AOT
//!   equivalent of "the preload file is not there" is a COMPILE ERROR, not a runtime one.
//!   `preload_verdict` makes that decision once (empty directive or disabled cache ⇒ nothing
//!   happens at all; set + enabled + unresolvable ⇒ compile error; set + enabled + resolvable
//!   ⇒ `preload_statistics` is emitted, with a compile warning when the file is outside the
//!   compile-time script manifest). See `preload_verdict` for the verified reference matrix and
//!   `render_preload_statistics_stmt` for the verified key shape.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::names::{canonical_name_for_decl, Name};
use crate::parser::ast::{BinOp, Expr, Program, Stmt, StmtKind};
use crate::synthetic_class::{
    e_array, e_array_assoc, e_binop, e_bool, e_call, e_float, e_int, e_str, e_var,
};
use crate::web_prelude::PhpVersion;

use crate::opcache::directives::{
    accel_hash_max_num_entries, directive_access, directive_env_type_code, directive_env_var_names,
    directive_ini_null_default, directive_runtime_overridable, effective_directive_ini_string,
    effective_jit_config, effective_opcache_directives, opcache_directives, opcache_version_string,
    DirectiveValue, OPCACHE_PRODUCT_NAME,
};
use crate::opcache::state::opcache_cache_enabled_with_overrides;

/// The reference-detection walk. Shared with `crate::version_prelude`, which needs exactly
/// the same "does this program mention this function name" question for its own pay-for-use
/// gating; duplicating the exhaustive AST traversal would be a second thing to keep correct.
pub(crate) mod detect;

/// The declaration SHAPES, built as AST. The modules below decide WHAT a binary needs and
/// compute the values baked into it; `build` spells out the bodies those values go into.
pub(crate) mod build;

mod manifest;
mod state_restriction;
mod preload;
mod restricted_status;
mod status_render;
mod scripts_configuration;
mod cli_ini;
mod env_ini;
mod injection;
mod manifest_bake;
#[cfg(test)]
mod tests;

#[allow(unused_imports)]
use manifest::*;
#[allow(unused_imports)]
use state_restriction::*;
#[allow(unused_imports)]
use preload::*;
#[allow(unused_imports)]
use restricted_status::*;
#[allow(unused_imports)]
use status_render::*;
#[allow(unused_imports)]
use scripts_configuration::*;
#[allow(unused_imports)]
use cli_ini::*;
#[allow(unused_imports)]
use env_ini::*;
#[allow(unused_imports)]
use injection::*;
#[allow(unused_imports)]
use manifest_bake::*;

pub use injection::inject_if_used;
pub use manifest::{collect_manifest, ScriptEntry};
pub use manifest_bake::{bake_manifest, ManifestBakeSites};
#[allow(unused_imports)]
pub use preload::{
    collect_preload_symbols, preload_statistics, preload_verdict, PreloadStatistics,
    PreloadSymbols, PreloadVerdict,
};
pub use state_restriction::canonical_entry_path;
pub(crate) use cli_ini::ini_module_known_declaration;
pub(crate) use env_ini::{env_override_declarations, ini_helper_declarations};
