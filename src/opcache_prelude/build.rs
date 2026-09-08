//! Purpose:
//! Builds the OPcache prelude's function declarations as AST, replacing the PHP-source
//! templates this module used to splice strings into. Each function here is one former
//! `*_TEMPLATE` constant, and every `__PLACEHOLDER__` it carried is now a Rust PARAMETER —
//! typed, named, and impossible to leave unfilled.
//!
//! Called from:
//! - `crate::opcache_prelude::inject_if_used` and `bake_manifest`, which decide WHICH
//!   declarations a binary needs and compute the values spliced into them.
//! - `crate::web_prelude`, for the three blocks a `--web` binary owns instead of the CLI one
//!   (the `opcache.*` INI dispatcher, the `ELEPHC_INI_*` environment helpers, and the
//!   known-module predicate).
//!
//! Key details:
//! - The bodies are the same PHP they always were; only their REPRESENTATION changed. Each
//!   builder was transcribed from the parse of the template it replaces and checked against it
//!   node for node before the text was deleted, so the behavioural docblocks that justified
//!   every line of those templates still live on the callers in `crate::opcache_prelude` —
//!   this module is deliberately just the shape.
//! - A placeholder that was a STATEMENT slot (`__RESTRICT_API_WARNING__`,
//!   `__PRELOAD_STATISTICS__`) is a `Vec<Stmt>`/`Option<Expr>` parameter, and one that was an
//!   ARRAY-ENTRY slot (`__INTERNED_STRINGS_USAGE__`) is an `Option<Expr>`. The text form
//!   expressed "omit this key" by deleting a line and its newline; here the key is simply not
//!   pushed, which is the same thing said once instead of twice.
//! - `path_normalization_stmts` is the one piece of deduplication the text form could not do:
//!   `opcache_is_script_cached`, `opcache_invalidate` and `opcache_compile_file` spelled the
//!   identical `realpath`/`getcwd` prologue out three times, and the `--strict-opcache`
//!   variant a fourth. Sharing it means the empty-path rule (see `INVALIDATE_TEMPLATE`'s
//!   successor docblock in the parent module) cannot drift between them.

use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_array_assoc, e_binop, e_bool, e_call, e_cast, e_const, e_float, e_index, e_int,
    e_neg, e_new, e_not, e_null, e_str, e_var, function, s_array_assign, s_assign, s_break,
    s_expr, s_foreach, s_if, s_return, s_static, s_throw, s_while, t_array, t_nullable, t_union,
};

/// A BAKED integer, written the way the parser produces it from decimal source text: a NEGATIVE
/// value is `Negate(IntLiteral(n))`, because PHP has no negative integer token — the sign is a
/// unary operator the parser does not fold. Every figure this module splices goes through here
/// rather than `e_int`, so a baked value keeps folding, narrowing and typing exactly as the
/// spliced text did. `opcache_get_status()`'s `free_memory` is the one that made it necessary:
/// `--ini opcache.memory_consumption=1` drives it negative.
///
/// `e_int` stays the right call for a literal that is part of the TEMPLATE (`0`, `7`, `48`) —
/// those are written here, not derived, and are never negative.
pub(crate) fn php_int(value: i64) -> Expr {
    match value.checked_neg() {
        Some(positive) if value < 0 => e_neg(e_int(positive)),
        // `i64::MIN` has no positive counterpart, and the lexer folds `PHP_INT_MIN` to the
        // literal itself rather than a negation, so the plain literal is also the parsed shape.
        _ => e_int(value),
    }
}

/// A KEYED array literal built from data, which may be empty.
///
/// `[]` is written the same way whether it would have held keyed or positional entries, and the
/// parser has only one node for it — `ArrayLiteral`, never `ArrayLiteralAssoc`. So a builder that
/// reaches for `e_array_assoc` unconditionally produces a node the source form cannot express as
/// soon as its data runs dry. `opcache_get_status()`'s `scripts` map is exactly that case: an
/// empty manifest.
pub(crate) fn php_assoc(entries: Vec<(Expr, Expr)>) -> Expr {
    if entries.is_empty() {
        return e_array(vec![]);
    }
    e_array_assoc(entries)
}

/// The VERBATIM `E_WARNING` php-src's OPcache API guard emits when `opcache.restrict_api`
/// denies a call, written straight to `STDERR` as `Warning: <text>`. See the parent module's
/// `RESTRICT_API_WARNING_TEXT` for the byte-for-byte reference evidence and why this does not
/// go through `trigger_error`.
pub(crate) fn restrict_api_warning_stmt(text: &str) -> Stmt {
    s_expr(e_call(
        "fwrite",
        vec![
            e_const("STDERR"),
            e_binop(e_str(text), BinOp::Concat, e_str("\n")),
        ],
    ))
}

/// The `$path` prologue the three path-taking OPcache functions share: an empty argument
/// resolves through `getcwd()`, anything else through `realpath()`, and an unresolvable path
/// leaves the function early with `false`.
fn path_normalization_stmts() -> Vec<Stmt> {
    vec![
        s_assign("path", e_str("")),
        s_if(
            e_binop(e_var("filename"), BinOp::StrictEq, e_str("")),
            vec![
                s_assign("cwd", e_call("getcwd", vec![])),
                s_if(
                    e_binop(e_var("cwd"), BinOp::StrictEq, e_bool(false)),
                    vec![s_return(e_bool(false))],
                    vec![],
                    None,
                ),
                s_assign("path", e_cast(CastType::String, e_var("cwd"))),
            ],
            vec![],
            Some(vec![
                s_assign("rp", e_call("realpath", vec![e_var("filename")])),
                s_if(
                    e_binop(e_var("rp"), BinOp::StrictEq, e_bool(false)),
                    vec![s_return(e_bool(false))],
                    vec![],
                    None,
                ),
                s_assign("path", e_cast(CastType::String, e_var("rp"))),
            ]),
        ),
    ]
}

/// `in_array($path, <manifest>, true)` — the strict membership test the manifest-carrying
/// functions run against the baked canonical-path list.
fn in_manifest(manifest_paths: Expr) -> Expr {
    e_call(
        "in_array",
        vec![e_var("path"), manifest_paths, e_bool(true)],
    )
}

/// `opcache_get_configuration()`: returns the baked configuration array.
pub(crate) fn get_configuration_decl(configuration: Expr) -> Stmt {
    function("opcache_get_configuration")
        .returns(t_array())
        .body(vec![s_return(configuration)])
        .build()
}

/// The `opcache.restrict_api`-denied `opcache_get_configuration()`: warning + `false`, with the
/// real configuration array kept as a DEAD arm so the inferred return stays `array|false` and a
/// caller's `is_array()` guard still narrows.
pub(crate) fn restricted_get_configuration_decl(configuration: Expr, warning: Stmt) -> Stmt {
    function("opcache_get_configuration")
        .body(vec![
            s_if(
                e_binop(e_bool(false), BinOp::StrictEq, e_bool(false)),
                vec![warning, s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_return(configuration),
        ])
        .build()
}

/// `opcache_reset()`: the compile-time enabled gate, then the one-shot restart latch.
pub(crate) fn reset_decl(enabled: bool) -> Stmt {
    function("opcache_reset")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_bool(enabled), BinOp::StrictEq, e_bool(false)),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_if(
                e_call("__elephc_opcache_restart_pending", vec![e_bool(false)]),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_assign(
                "scheduled",
                e_call("__elephc_opcache_restart_pending", vec![e_bool(true)]),
            ),
            s_return(e_var("scheduled")),
        ])
        .build()
}

/// The restricted `opcache_reset()`: warning + `false`.
pub(crate) fn restricted_reset_decl(warning: Stmt) -> Stmt {
    function("opcache_reset")
        .returns(TypeExpr::Bool)
        .body(vec![warning, s_return(e_bool(false))])
        .build()
}

/// Everything `opcache_get_status()`'s body varies on. Each field was a `__…__` placeholder in
/// the template this replaces; the two `Option`s are the keys reference PHP OMITS rather than
/// zeroes (see the parent module's `splice_interned_strings_usage` /
/// `splice_preload_statistics` successors).
pub(crate) struct StatusFacts {
    /// The compile-time cache-enabled gate. A restricted API forces it `false`.
    pub enabled: bool,
    /// The `opcache.restrict_api` diagnostic, present only when the API is denied.
    pub warning: Option<Stmt>,
    /// `memory_usage.used_memory` — the baseline plus Σ per-script memory.
    pub memory_used: i64,
    /// `memory_usage.free_memory` — `memory_consumption - used - wasted`, with `wasted = 0`.
    pub memory_free: i64,
    /// The whole `interned_strings_usage` value, or `None` when the buffer was never stood up
    /// (php-src omits the KEY, so `None` is absence, not a zeroed sub-array).
    pub interned_strings_usage: Option<Expr>,
    /// `opcache_statistics.num_cached_scripts` — one per manifest entry.
    pub num_cached_scripts: i64,
    /// `opcache_statistics.num_cached_keys` — one key per script in this model.
    pub num_cached_keys: i64,
    /// `opcache_statistics.max_cached_keys` — OPcache's prime-rounded hash capacity.
    pub max_cached_keys: i64,
    /// The `preload_statistics` value, or `None` when this binary does not preload.
    pub preload_statistics: Option<Expr>,
    /// The `scripts` map keyed by canonical path.
    pub scripts_map: Expr,
    /// The `jit` sub-array's seven fields, already clamped.
    pub jit: JitFacts,
}

/// The seven `jit` fields of `opcache_get_status()`, as the parent module derives them.
pub(crate) struct JitFacts {
    /// Always `false`: an AOT binary has no runtime JIT engine.
    pub enabled: bool,
    /// Always `false`, for the same reason.
    pub on: bool,
    /// The CONFIGURED `opcache.jit` kind, reported verbatim.
    pub kind: i64,
    /// The configured optimization level.
    pub opt_level: i64,
    /// The configured optimization flags.
    pub opt_flags: i64,
    /// Always 0: no JIT buffer is ever allocated.
    pub buffer_size: i64,
    /// Always 0, for the same reason.
    pub buffer_free: i64,
}

/// `opcache_get_status($include_scripts = true)`.
///
/// The return type hint is deliberately absent (reference PHP is `array|false`) so ordinary
/// union return inference handles the two exits. Key ORDER is load-bearing:
/// `preload_statistics` sits between `opcache_statistics` and `scripts`, and `jit` is last.
pub(crate) fn get_status_decl(facts: StatusFacts) -> Stmt {
    let mut gate_body = Vec::new();
    if let Some(warning) = facts.warning {
        gate_body.push(warning);
    }
    gate_body.push(s_return(e_bool(false)));

    let mut status_entries = vec![
        (e_str("opcache_enabled"), e_bool(true)),
        (e_str("cache_full"), e_bool(false)),
        (
            e_str("restart_pending"),
            e_call("__elephc_opcache_restart_pending", vec![e_bool(false)]),
        ),
        (e_str("restart_in_progress"), e_bool(false)),
        (
            e_str("memory_usage"),
            e_array_assoc(vec![
                (e_str("used_memory"), php_int(facts.memory_used)),
                (e_str("free_memory"), php_int(facts.memory_free)),
                (e_str("wasted_memory"), e_int(0)),
                (e_str("current_wasted_percentage"), e_float(0.0)),
            ]),
        ),
    ];
    if let Some(usage) = facts.interned_strings_usage {
        status_entries.push((e_str("interned_strings_usage"), usage));
    }
    status_entries.push((
        e_str("opcache_statistics"),
        e_array_assoc(vec![
            (
                e_str("num_cached_scripts"),
                php_int(facts.num_cached_scripts),
            ),
            (e_str("num_cached_keys"), php_int(facts.num_cached_keys)),
            (e_str("max_cached_keys"), php_int(facts.max_cached_keys)),
            (e_str("hits"), e_int(0)),
            (
                e_str("start_time"),
                e_var("__elephc_opcache_start_time"),
            ),
            (e_str("last_restart_time"), e_int(0)),
            (e_str("oom_restarts"), e_int(0)),
            (e_str("hash_restarts"), e_int(0)),
            (e_str("manual_restarts"), e_int(0)),
            (e_str("misses"), e_int(0)),
            (e_str("blacklist_misses"), e_int(0)),
            (e_str("blacklist_miss_ratio"), e_float(0.0)),
            (e_str("opcache_hit_rate"), e_float(0.0)),
        ]),
    ));

    let mut body = vec![
        s_if(
            e_binop(e_bool(facts.enabled), BinOp::StrictEq, e_bool(false)),
            gate_body,
            vec![],
            None,
        ),
        // The cache-start instant, memoized: reference PHP reports one fixed point for the life
        // of the process, and this same clock stamps every `scripts` entry.
        s_static("__elephc_opcache_start_time", e_int(0)),
        s_if(
            e_binop(
                e_var("__elephc_opcache_start_time"),
                BinOp::StrictEq,
                e_int(0),
            ),
            vec![s_assign(
                "__elephc_opcache_start_time",
                e_call("time", vec![]),
            )],
            vec![],
            None,
        ),
        s_assign("status", e_array_assoc(status_entries)),
    ];
    if let Some(preload) = facts.preload_statistics {
        body.push(s_array_assign(
            "status",
            e_str("preload_statistics"),
            preload,
        ));
    }
    body.push(s_if(
        e_var("include_scripts"),
        vec![s_array_assign(
            "status",
            e_str("scripts"),
            facts.scripts_map,
        )],
        vec![],
        None,
    ));
    body.push(s_array_assign(
        "status",
        e_str("jit"),
        e_array_assoc(vec![
            (e_str("enabled"), e_bool(facts.jit.enabled)),
            (e_str("on"), e_bool(facts.jit.on)),
            (e_str("kind"), php_int(facts.jit.kind)),
            (e_str("opt_level"), php_int(facts.jit.opt_level)),
            (e_str("opt_flags"), php_int(facts.jit.opt_flags)),
            (e_str("buffer_size"), php_int(facts.jit.buffer_size)),
            (e_str("buffer_free"), php_int(facts.jit.buffer_free)),
        ]),
    ));
    body.push(s_return(e_var("status")));

    function("opcache_get_status")
        .param_untyped_default("include_scripts", e_bool(true))
        .body(body)
        .build()
}

/// `opcache_is_script_cached($filename)`: `realpath`-normalized membership in the baked
/// manifest, with a force-invalidated entry reporting `false`.
pub(crate) fn is_script_cached_decl(enabled: bool, manifest_paths: Expr) -> Stmt {
    let mut body = vec![s_if(
        e_binop(e_bool(enabled), BinOp::StrictEq, e_bool(false)),
        vec![s_return(e_bool(false))],
        vec![],
        None,
    )];
    body.extend(path_normalization_stmts());
    body.push(s_if(
        e_call(
            "__elephc_opcache_invalidate_state",
            vec![e_var("path"), e_int(0)],
        ),
        vec![s_return(e_bool(false))],
        vec![],
        None,
    ));
    body.push(s_return(in_manifest(manifest_paths)));
    function("opcache_is_script_cached")
        .param_untyped("filename")
        .returns(TypeExpr::Bool)
        .body(body)
        .build()
}

/// The restricted `opcache_is_script_cached()`: warning + `false`, with `$filename` consumed
/// first so the checker does not report the parameter unused.
pub(crate) fn restricted_is_script_cached_decl(warning: Stmt) -> Stmt {
    function("opcache_is_script_cached")
        .param_untyped("filename")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("filename", e_cast(CastType::String, e_var("filename"))),
            warning,
            s_return(e_bool(false)),
        ])
        .build()
}

/// `opcache_invalidate($filename, $force = false)`.
///
/// `strict` selects the `--strict-opcache` variant, whose only difference is what a FORCED call
/// on a manifest member does: record the discard, or throw because code frozen into the binary
/// cannot be reloaded from disk.
pub(crate) fn invalidate_decl(enabled: bool, manifest_paths: Expr, strict: bool) -> Stmt {
    let forced_action = if strict {
        s_throw(e_new(
            "RuntimeException",
            vec![e_binop(
                e_binop(
                    e_str("opcache_invalidate(): --strict-opcache: cannot invalidate \""),
                    BinOp::Concat,
                    e_var("path"),
                ),
                BinOp::Concat,
                e_str("\": it is compiled into this binary and cannot be reloaded from disk"),
            )],
        ))
    } else {
        s_expr(e_call(
            "__elephc_opcache_invalidate_state",
            vec![e_var("path"), e_int(1)],
        ))
    };

    let mut body = vec![
        s_if(
            e_binop(e_bool(enabled), BinOp::StrictEq, e_bool(false)),
            vec![s_return(e_bool(false))],
            vec![],
            None,
        ),
        s_assign("force", e_cast(CastType::Bool, e_var("force"))),
    ];
    body.extend(path_normalization_stmts());
    body.push(s_if(
        e_binop(e_var("force"), BinOp::And, in_manifest(manifest_paths)),
        vec![forced_action],
        vec![],
        None,
    ));
    body.push(s_return(e_bool(true)));

    function("opcache_invalidate")
        .param_untyped("filename")
        .param_untyped_default("force", e_bool(false))
        .returns(TypeExpr::Bool)
        .body(body)
        .build()
}

/// The restricted `opcache_invalidate()`: warning + `false`, both parameters consumed first.
pub(crate) fn restricted_invalidate_decl(warning: Stmt) -> Stmt {
    function("opcache_invalidate")
        .param_untyped("filename")
        .param_untyped_default("force", e_bool(false))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("filename", e_cast(CastType::String, e_var("filename"))),
            s_assign("force", e_cast(CastType::Bool, e_var("force"))),
            warning,
            s_return(e_bool(false)),
        ])
        .build()
}

/// `opcache_compile_file($filename)`: `true` for a manifest member (already compiled into the
/// binary), `false` otherwise, and an engine-level notice on the disabled path.
pub(crate) fn compile_file_decl(enabled: bool, manifest_paths: Expr) -> Stmt {
    let mut body = vec![s_if(
        e_binop(e_bool(enabled), BinOp::StrictEq, e_bool(false)),
        vec![
            s_expr(e_call(
                "fwrite",
                vec![
                    e_const("STDERR"),
                    e_str("Notice: Zend OPcache has not been properly started, can't compile file\n"),
                ],
            )),
            s_return(e_bool(false)),
        ],
        vec![],
        None,
    )];
    body.extend(path_normalization_stmts());
    body.push(s_if(
        e_not(in_manifest(manifest_paths)),
        vec![s_return(e_bool(false))],
        vec![],
        None,
    ));
    // Re-caching clears a forced invalidation UNLESS a restart is pending: php-src only stores
    // into the cache while the shared accelerator flag holds.
    body.push(s_if(
        e_not(e_call(
            "__elephc_opcache_restart_pending",
            vec![e_bool(false)],
        )),
        vec![s_expr(e_call(
            "__elephc_opcache_invalidate_state",
            vec![e_var("path"), e_int(2)],
        ))],
        vec![],
        None,
    ));
    body.push(s_return(e_bool(true)));

    function("opcache_compile_file")
        .param_untyped("filename")
        .returns(TypeExpr::Bool)
        .body(body)
        .build()
}

/// `opcache_is_script_cached_in_file_cache($filename)`: always `false`, which is EXACT — php-src
/// gates the whole function on `opcache.file_cache`, which has no default.
pub(crate) fn is_script_cached_in_file_cache_decl() -> Stmt {
    function("opcache_is_script_cached_in_file_cache")
        .param_untyped("filename")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("filename", e_cast(CastType::String, e_var("filename"))),
            s_return(e_bool(false)),
        ])
        .build()
}

/// The restricted `opcache_is_script_cached_in_file_cache()`: the WARNING is the only thing that
/// distinguishes it from the normal body, which is why it has to exist separately.
pub(crate) fn restricted_is_script_cached_in_file_cache_decl(warning: Stmt) -> Stmt {
    function("opcache_is_script_cached_in_file_cache")
        .param_untyped("filename")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("filename", e_cast(CastType::String, e_var("filename"))),
            warning,
            s_return(e_bool(false)),
        ])
        .build()
}

/// `opcache_jit_blacklist($closure)`: a no-op returning `null`. An AOT binary has no runtime JIT
/// blacklist to add to.
pub(crate) fn jit_blacklist_decl() -> Stmt {
    function("opcache_jit_blacklist")
        .param_untyped("closure")
        .returns(TypeExpr::Void)
        .body(vec![s_assign("closure", e_var("closure"))])
        .build()
}

/// The in-process OPcache state block: the restart latch, the discard set, the per-script
/// timestamp projection, and the two clock helpers the `scripts` map's `last_used` needs.
pub(crate) fn state_helper_decls() -> Program {
    vec![
        function("__elephc_opcache_restart_pending")
            .param("schedule", TypeExpr::Bool)
            .returns(TypeExpr::Bool)
            .body(vec![
                s_static("pending", e_bool(false)),
                s_if(
                    e_var("schedule"),
                    vec![s_assign("pending", e_bool(true))],
                    vec![],
                    None,
                ),
                s_return(e_var("pending")),
            ])
            .build(),
        function("__elephc_opcache_invalidate_state")
            .param("path", TypeExpr::Str)
            .param("op", TypeExpr::Int)
            .returns(TypeExpr::Bool)
            .body(vec![
                // Seeded with one typed dummy entry: the EIR backend rejects `static $s = [];`.
                s_static(
                    "discarded",
                    e_array_assoc(vec![(e_str(""), e_bool(false))]),
                ),
                s_if(
                    e_binop(e_var("op"), BinOp::StrictEq, e_int(1)),
                    vec![s_array_assign("discarded", e_var("path"), e_bool(true))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("op"), BinOp::StrictEq, e_int(2)),
                    vec![s_array_assign("discarded", e_var("path"), e_bool(false))],
                    vec![],
                    None,
                ),
                // An `isset` guard plus a bound local, not `?? false`: the coalesce reads `true`
                // for an absent key in elephc.
                s_if(
                    e_not(e_call(
                        "isset",
                        vec![e_index(e_var("discarded"), e_var("path"))],
                    )),
                    vec![s_return(e_bool(false))],
                    vec![],
                    None,
                ),
                s_assign("state", e_index(e_var("discarded"), e_var("path"))),
                s_return(e_var("state")),
            ])
            .build(),
        function("__elephc_opcache_script_timestamp")
            .param("path", TypeExpr::Str)
            .param("timestamp", TypeExpr::Int)
            .returns(TypeExpr::Int)
            .body(vec![
                s_if(
                    e_call(
                        "__elephc_opcache_invalidate_state",
                        vec![e_var("path"), e_int(0)],
                    ),
                    vec![s_return(e_int(0))],
                    vec![],
                    None,
                ),
                s_return(e_var("timestamp")),
            ])
            .build(),
        function("__elephc_opcache_system_timezone")
            .returns(TypeExpr::Str)
            .body(vec![
                s_assign(
                    "tz",
                    e_cast(CastType::String, e_call("getenv", vec![e_str("TZ")])),
                ),
                s_if(
                    e_binop(e_var("tz"), BinOp::StrictNotEq, e_str("")),
                    vec![s_return(e_var("tz"))],
                    vec![],
                    None,
                ),
                s_assign(
                    "link",
                    e_call("readlink", vec![e_str("/etc/localtime")]),
                ),
                s_if(
                    e_binop(e_var("link"), BinOp::StrictEq, e_bool(false)),
                    vec![s_return(e_str(""))],
                    vec![],
                    None,
                ),
                s_assign("target", e_cast(CastType::String, e_var("link"))),
                s_assign(
                    "at",
                    e_call("strpos", vec![e_var("target"), e_str("/zoneinfo/")]),
                ),
                s_if(
                    e_binop(e_var("at"), BinOp::StrictEq, e_bool(false)),
                    vec![s_return(e_str(""))],
                    vec![],
                    None,
                ),
                s_assign(
                    "zone",
                    e_call(
                        "substr",
                        vec![
                            e_var("target"),
                            e_binop(e_var("at"), BinOp::Add, e_int(10)),
                        ],
                    ),
                ),
                s_return(e_var("zone")),
            ])
            .build(),
        function("__elephc_opcache_asctime")
            .param("timestamp", TypeExpr::Int)
            .returns(TypeExpr::Str)
            .body(vec![
                s_assign(
                    "zone",
                    e_call("__elephc_opcache_system_timezone", vec![]),
                ),
                s_assign("previous", e_str("")),
                s_if(
                    e_binop(e_var("zone"), BinOp::StrictNotEq, e_str("")),
                    vec![
                        s_assign(
                            "previous",
                            e_call("date_default_timezone_get", vec![]),
                        ),
                        s_expr(e_call(
                            "date_default_timezone_set",
                            vec![e_var("zone")],
                        )),
                    ],
                    vec![],
                    None,
                ),
                s_assign(
                    "day",
                    e_cast(
                        CastType::Int,
                        e_call("date", vec![e_str("j"), e_var("timestamp")]),
                    ),
                ),
                // `asctime`'s day-of-month is `%3d` — SPACE-padded.
                s_assign("pad", e_str("")),
                s_if(
                    e_binop(e_var("day"), BinOp::Lt, e_int(10)),
                    vec![s_assign("pad", e_str(" "))],
                    vec![],
                    None,
                ),
                s_assign(
                    "formatted",
                    e_binop(
                        e_binop(
                            e_binop(
                                e_call("date", vec![e_str("D M "), e_var("timestamp")]),
                                BinOp::Concat,
                                e_var("pad"),
                            ),
                            BinOp::Concat,
                            e_var("day"),
                        ),
                        BinOp::Concat,
                        e_call("date", vec![e_str(" H:i:s Y"), e_var("timestamp")]),
                    ),
                ),
                s_if(
                    e_binop(e_var("zone"), BinOp::StrictNotEq, e_str("")),
                    vec![s_expr(e_call(
                        "date_default_timezone_set",
                        vec![e_var("previous")],
                    ))],
                    vec![],
                    None,
                ),
                s_return(e_var("formatted")),
            ])
            .build(),
    ]
}

/// The CLI `ini_get(string $option): string|false` wrapper.
pub(crate) fn cli_ini_get_decl() -> Stmt {
    function("ini_get")
        .param("option", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("date.timezone")),
                vec![s_return(e_call("date_default_timezone_get", vec![]))],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_opcache_ini_string", vec![e_var("option")])),
        ])
        .build()
}

/// Builds the internal line-aware `ini_set` dispatch helper and its public two-argument wrapper.
pub(crate) fn cli_ini_set_decls() -> Vec<Stmt> {
    let suppressed_set = Expr::new(
        ExprKind::ErrorSuppress(Box::new(e_call(
            "date_default_timezone_set",
            vec![e_var("value")],
        ))),
        crate::span::Span::dummy(),
    );
    let invalid_message = e_binop(
        e_binop(
            e_binop(
                e_str("\nWarning: ini_set(): Invalid date.timezone value '"),
                BinOp::Concat,
                e_var("value"),
            ),
            BinOp::Concat,
            e_str("', using '"),
        ),
        BinOp::Concat,
        e_binop(e_var("old"), BinOp::Concat, e_str("' instead")),
    );
    let dispatch = function(super::CLI_INI_SET_DISPATCH_HELPER)
        .param("option", TypeExpr::Str)
        .param_untyped("value")
        .param_default("line", TypeExpr::Int, e_int(0))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("value", e_cast(CastType::String, e_var("value"))),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("date.timezone")),
                vec![
                    s_assign("old", e_call("date_default_timezone_get", vec![])),
                    s_if(
                        e_not(suppressed_set),
                        vec![
                            s_assign("message", invalid_message),
                            s_if(
                                e_binop(e_var("line"), BinOp::Gt, e_int(0)),
                                vec![s_expr(e_call(
                                    "__elephc_diag_warning",
                                    vec![e_var("message"), e_var("line")],
                                ))],
                                vec![],
                                Some(vec![s_expr(e_call(
                                    "__elephc_diag_warning",
                                    vec![e_binop(
                                        e_var("message"),
                                        BinOp::Concat,
                                        e_str("\n"),
                                    )],
                                ))]),
                            ),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("old")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_call("__elephc_opcache_ini_string", vec![e_var("option")]),
                    BinOp::StrictEq,
                    e_var("value"),
                ),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_return(e_bool(false)),
        ])
        .build();
    let public = function("ini_set")
        .param("option", TypeExpr::Str)
        .param_untyped("value")
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![s_return(e_call(
            super::CLI_INI_SET_DISPATCH_HELPER,
            vec![e_var("option"), e_var("value"), e_int(0)],
        ))])
        .build();
    vec![dispatch, public]
}

/// The CLI `ini_get_all(?string $extension = null, bool $details = true)` wrapper — the
/// extension-filter dispatch, byte-modeled on php-src. The return type hint is deliberately
/// omitted; the `$details` split happens HERE rather than inside a shared loop, because a
/// function writing an array on one branch and a scalar on the other into the SAME slot
/// miscompiles.
pub(crate) fn cli_ini_get_all_decl() -> Stmt {
    function("ini_get_all")
        .param_default("extension", t_nullable(TypeExpr::Str), e_null())
        .param_default("details", TypeExpr::Bool, e_bool(true))
        .body(vec![
            s_if(
                e_binop(
                    e_binop(
                        e_binop(e_var("extension"), BinOp::StrictNotEq, e_null()),
                        BinOp::And,
                        e_binop(
                            e_var("extension"),
                            BinOp::StrictNotEq,
                            e_str("zend opcache"),
                        ),
                    ),
                    BinOp::And,
                    e_binop(e_var("extension"), BinOp::StrictNotEq, e_str("core")),
                ),
                vec![
                    s_if(
                        e_call("__elephc_ini_module_known", vec![e_var("extension")]),
                        vec![s_return(e_array(vec![]))],
                        vec![],
                        None,
                    ),
                    s_expr(e_call(
                        "fwrite",
                        vec![
                            e_const("STDERR"),
                            e_binop(
                                e_binop(
                                    e_binop(
                                        e_str("Warning: ini_get_all(): Extension \""),
                                        BinOp::Concat,
                                        e_var("extension"),
                                    ),
                                    BinOp::Concat,
                                    e_str("\" cannot be found"),
                                ),
                                BinOp::Concat,
                                e_str("\n"),
                            ),
                        ],
                    )),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("details"),
                vec![s_return(e_call(
                    "__elephc_opcache_ini_all_details",
                    vec![],
                ))],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_opcache_ini_all_plain", vec![])),
        ])
        .build()
}

/// `__elephc_ini_module_known(?string $m): bool` — the known-module predicate `ini_get_all`'s
/// extension filter uses to tell "known module with no INI directives" (`[]`) from "no such
/// module" (`E_WARNING` + `false`). `names` is compared VERBATIM: reference PHP does not
/// case-fold this argument.
///
/// `?string` rather than `string` because `$extension !== null` does not currently narrow a
/// `?string` parameter to `Str` in the checker.
pub(crate) fn ini_module_known_decl(names: &[String]) -> Stmt {
    function("__elephc_ini_module_known")
        .param("m", t_nullable(TypeExpr::Str))
        .returns(TypeExpr::Bool)
        .body(vec![s_return(or_chain(
            names
                .iter()
                .map(|name| e_binop(e_var("m"), BinOp::StrictEq, e_str(name)))
                .collect(),
        ))])
        .build()
}

/// Folds `a || b || c` the way PHP parses it — LEFT-associative, so the tree matches what the
/// source form produced. An empty list would have no PHP spelling at all, and every caller
/// derives its terms from a non-empty compile-time table, so it panics rather than inventing one.
pub(crate) fn or_chain(terms: Vec<Expr>) -> Expr {
    let mut terms = terms.into_iter();
    let first = terms
        .next()
        .expect("an || chain needs at least one condition");
    terms.fold(first, |left, right| e_binop(left, BinOp::Or, right))
}

/// The `opcache.*` INI helper block: the raw-string dispatcher, the NULL-default predicate, the
/// detail-value projection, the access bitmask, the sorted key list, and the two whole-block
/// helpers.
///
/// `string_arms` and `null_arms` are the per-directive `if ($option === '<name>') { return …; }`
/// bodies; `all_names` is the PHP_INI_ALL directive set the access bitmask answers `7` for; and
/// `keys` is the ASCENDING-sorted key list `ini_get_all` reports.
///
/// TWO ALL-HELPERS, NOT ONE `$details` LOOP — see `cli_ini_get_all_decl`.
pub(crate) fn ini_helper_decls(
    string_arms: Vec<(String, Expr)>,
    null_arms: Vec<(String, Expr)>,
    all_names: &[&str],
    keys: &[&str],
) -> Program {
    let arm = |(name, value): (String, Expr)| {
        s_if(
            e_binop(e_var("option"), BinOp::StrictEq, e_str(&name)),
            vec![s_return(value)],
            vec![],
            None,
        )
    };

    let mut string_body: Vec<Stmt> = string_arms.into_iter().map(arm).collect();
    string_body.push(s_return(e_bool(false)));

    let mut null_body: Vec<Stmt> = null_arms.into_iter().map(arm).collect();
    null_body.push(s_return(e_bool(false)));

    vec![
        function("__elephc_opcache_ini_string")
            .param("option", TypeExpr::Str)
            .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
            .body(string_body)
            .build(),
        function("__elephc_opcache_ini_null")
            .param("option", TypeExpr::Str)
            .returns(TypeExpr::Bool)
            .body(null_body)
            .build(),
        // The `?string` return hint is LOAD-BEARING: without it elephc infers plain `Str` and
        // coerces the `return null` to `''`.
        function("__elephc_opcache_ini_detail_value")
            .param("option", TypeExpr::Str)
            .returns(t_nullable(TypeExpr::Str))
            .body(vec![
                s_if(
                    e_call("__elephc_opcache_ini_null", vec![e_var("option")]),
                    vec![s_return(e_null())],
                    vec![],
                    None,
                ),
                s_assign(
                    "__elephc_raw",
                    e_cast(
                        CastType::String,
                        e_call("__elephc_opcache_ini_string", vec![e_var("option")]),
                    ),
                ),
                s_return(e_var("__elephc_raw")),
            ])
            .build(),
        function("__elephc_opcache_ini_access")
            .param("option", TypeExpr::Str)
            .returns(TypeExpr::Int)
            .body(vec![
                s_if(
                    e_binop(
                        e_call("__elephc_opcache_ini_string", vec![e_var("option")]),
                        BinOp::StrictEq,
                        e_bool(false),
                    ),
                    vec![s_return(e_neg(e_int(1)))],
                    vec![],
                    None,
                ),
                s_if(
                    or_chain(
                        all_names
                            .iter()
                            .map(|name| {
                                e_binop(e_var("option"), BinOp::StrictEq, e_str(name))
                            })
                            .collect(),
                    ),
                    vec![s_return(e_int(7))],
                    vec![],
                    None,
                ),
                s_return(e_int(4)),
            ])
            .build(),
        function("__elephc_opcache_ini_keys")
            .returns(t_array())
            .body(vec![s_return(e_array(
                keys.iter().map(|name| e_str(name)).collect(),
            ))])
            .build(),
        function("__elephc_opcache_ini_all_details")
            .returns(t_array())
            .body(vec![
                s_assign("__elephc_all", e_array(vec![])),
                s_foreach(
                    e_call("__elephc_opcache_ini_keys", vec![]),
                    None,
                    "__elephc_k",
                    vec![
                        s_assign(
                            "__elephc_v",
                            e_call(
                                "__elephc_opcache_ini_detail_value",
                                vec![e_var("__elephc_k")],
                            ),
                        ),
                        s_array_assign(
                            "__elephc_all",
                            e_var("__elephc_k"),
                            e_array_assoc(vec![
                                (e_str("global_value"), e_var("__elephc_v")),
                                (e_str("local_value"), e_var("__elephc_v")),
                                (
                                    e_str("access"),
                                    e_call(
                                        "__elephc_opcache_ini_access",
                                        vec![e_var("__elephc_k")],
                                    ),
                                ),
                            ]),
                        ),
                    ],
                ),
                s_return(e_var("__elephc_all")),
            ])
            .build(),
        function("__elephc_opcache_ini_all_plain")
            .returns(t_array())
            .body(vec![
                s_assign("__elephc_all", e_array(vec![])),
                s_foreach(
                    e_call("__elephc_opcache_ini_keys", vec![]),
                    None,
                    "__elephc_k",
                    vec![s_array_assign(
                        "__elephc_all",
                        e_var("__elephc_k"),
                        e_call(
                            "__elephc_opcache_ini_detail_value",
                            vec![e_var("__elephc_k")],
                        ),
                    )],
                ),
                s_return(e_var("__elephc_all")),
            ])
            .build(),
    ]
}

/// The RUNTIME per-directive environment-override helper block (`ELEPHC_INI_*`): the lookup, the
/// scanner (the PHP mirror of `ini_scanner_value`), the per-type normalizers (the mirror of
/// `parse_ini_override`), and the typed plus raw-string surfaces those feed.
///
/// It carries no per-target data — the per-directive facts are the arguments its call sites pass
/// — so it is a constant declaration set. See the parent module's `render_opcache_env_helpers`
/// successor for why it is PHP rather than Rust and where it is injected.
pub(crate) fn env_override_helper_decls() -> Program {
    vec![
        function("__elephc_opcache_env")
            .param("u", TypeExpr::Str)
            .param("d", TypeExpr::Str)
            .returns(TypeExpr::Str)
            .body(vec![
                s_assign(
                    "v",
                    e_cast(CastType::String, e_call("getenv", vec![e_var("u")])),
                ),
                s_if(
                    e_binop(e_var("v"), BinOp::StrictNotEq, e_str("")),
                    vec![s_return(e_var("v"))],
                    vec![],
                    None,
                ),
                s_return(e_cast(
                    CastType::String,
                    e_call("getenv", vec![e_var("d")]),
                )),
            ])
            .build(),
        function("__elephc_ini_scan")
            .param("v", TypeExpr::Str)
            .returns(TypeExpr::Str)
            .body(vec![
                s_assign(
                    "l",
                    e_call("strtolower", vec![e_call("trim", vec![e_var("v")])]),
                ),
                s_if(
                    or_chain(
                        ["on", "true", "yes"]
                            .iter()
                            .map(|word| e_binop(e_var("l"), BinOp::StrictEq, e_str(word)))
                            .collect(),
                    ),
                    vec![s_return(e_str("1"))],
                    vec![],
                    None,
                ),
                s_if(
                    or_chain(
                        ["off", "false", "no", "none", "null"]
                            .iter()
                            .map(|word| e_binop(e_var("l"), BinOp::StrictEq, e_str(word)))
                            .collect(),
                    ),
                    vec![s_return(e_str(""))],
                    vec![],
                    None,
                ),
                s_return(e_var("v")),
            ])
            .build(),
        function("__elephc_ini_bool_val")
            .param("v", TypeExpr::Str)
            .returns(TypeExpr::Bool)
            .body(vec![
                s_assign("l", e_call("strtolower", vec![e_var("v")])),
                s_if(
                    or_chain(
                        ["true", "yes", "on"]
                            .iter()
                            .map(|word| e_binop(e_var("l"), BinOp::StrictEq, e_str(word)))
                            .collect(),
                    ),
                    vec![s_return(e_bool(true))],
                    vec![],
                    None,
                ),
                s_return(e_binop(
                    e_call("__elephc_ini_atoi", vec![e_var("v")]),
                    BinOp::StrictNotEq,
                    e_int(0),
                )),
            ])
            .build(),
        function("__elephc_ini_isspace")
            .param("c", TypeExpr::Str)
            .returns(TypeExpr::Bool)
            .body(vec![
                s_assign("o", e_call("ord", vec![e_var("c")])),
                s_return(e_binop(
                    e_binop(e_var("o"), BinOp::StrictEq, e_int(32)),
                    BinOp::Or,
                    e_binop(
                        e_binop(e_var("o"), BinOp::GtEq, e_int(9)),
                        BinOp::And,
                        e_binop(e_var("o"), BinOp::LtEq, e_int(13)),
                    ),
                )),
            ])
            .build(),
        function("__elephc_ini_digit")
            .param("c", TypeExpr::Str)
            .param("radix", TypeExpr::Int)
            .returns(TypeExpr::Int)
            .body(vec![
                s_assign("o", e_call("ord", vec![e_var("c")])),
                s_assign("d", e_neg(e_int(1))),
                digit_range(48, 57, 48),
                digit_range(97, 122, 87),
                digit_range(65, 90, 55),
                s_if(
                    e_binop(
                        e_binop(e_var("d"), BinOp::Lt, e_int(0)),
                        BinOp::Or,
                        e_binop(e_var("d"), BinOp::GtEq, e_var("radix")),
                    ),
                    vec![s_return(e_neg(e_int(1)))],
                    vec![],
                    None,
                ),
                s_return(e_var("d")),
            ])
            .build(),
        ini_quantity_decl(),
        function("__elephc_ini_atoi")
            .param("v", TypeExpr::Str)
            .returns(TypeExpr::Int)
            .body(vec![
                s_assign("s", e_call("ltrim", vec![e_var("v")])),
                s_assign("n", e_call("strlen", vec![e_var("s")])),
                s_assign("i", e_int(0)),
                s_assign("neg", e_bool(false)),
                s_assign(
                    "c",
                    e_call("substr", vec![e_var("s"), e_int(0), e_int(1)]),
                ),
                s_if(
                    e_binop(e_var("c"), BinOp::StrictEq, e_str("-")),
                    vec![s_assign("neg", e_bool(true)), s_assign("i", e_int(1))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("c"), BinOp::StrictEq, e_str("+")),
                    vec![s_assign("i", e_int(1))],
                    vec![],
                    None,
                ),
                s_assign("acc", e_int(0)),
                s_assign("seen", e_int(0)),
                s_while(
                    e_binop(e_var("i"), BinOp::Lt, e_var("n")),
                    vec![
                        s_assign(
                            "o",
                            e_call(
                                "ord",
                                vec![e_call(
                                    "substr",
                                    vec![e_var("s"), e_var("i"), e_int(1)],
                                )],
                            ),
                        ),
                        s_if(
                            e_binop(
                                e_binop(e_var("o"), BinOp::Lt, e_int(48)),
                                BinOp::Or,
                                e_binop(e_var("o"), BinOp::Gt, e_int(57)),
                            ),
                            vec![s_break(1)],
                            vec![],
                            None,
                        ),
                        // The 18-digit floor is where the accumulator stops mattering; php-src
                        // saturates, and no directive value reaches it.
                        s_if(
                            e_binop(e_var("seen"), BinOp::Lt, e_int(18)),
                            vec![s_assign(
                                "acc",
                                e_binop(
                                    e_binop(e_var("acc"), BinOp::Mul, e_int(10)),
                                    BinOp::Add,
                                    e_binop(e_var("o"), BinOp::Sub, e_int(48)),
                                ),
                            )],
                            vec![],
                            None,
                        ),
                        s_assign("seen", e_binop(e_var("seen"), BinOp::Add, e_int(1))),
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                    ],
                ),
                s_if(
                    e_var("neg"),
                    vec![s_return(e_neg(e_var("acc")))],
                    vec![],
                    None,
                ),
                s_return(e_var("acc")),
            ])
            .build(),
        function("__elephc_ini_pct_ok")
            .param("v", TypeExpr::Str)
            .returns(TypeExpr::Bool)
            .body(vec![
                s_assign("p", e_call("__elephc_ini_atoi", vec![e_var("v")])),
                s_return(e_binop(
                    e_binop(e_var("p"), BinOp::Gt, e_int(0)),
                    BinOp::And,
                    e_binop(e_var("p"), BinOp::LtEq, e_int(50)),
                )),
            ])
            .build(),
        function("__elephc_ini_pct_val")
            .param("v", TypeExpr::Str)
            .returns(TypeExpr::Float)
            .body(vec![s_return(e_binop(
                e_call("__elephc_ini_atoi", vec![e_var("v")]),
                BinOp::Div,
                e_float(100.0),
            ))])
            .build(),
        // The typed surface feeding `opcache_get_configuration()['directives']`. Each helper is
        // lookup → empty-means-unset floor → scan → its own normalizer.
        typed_env_helper(
            "__elephc_opcache_env_bool",
            TypeExpr::Bool,
            e_call(
                "__elephc_ini_bool_val",
                vec![e_call("__elephc_ini_scan", vec![e_var("v")])],
            ),
        ),
        typed_env_helper(
            "__elephc_opcache_env_int",
            TypeExpr::Int,
            e_call(
                "__elephc_ini_quantity",
                vec![e_call("__elephc_ini_scan", vec![e_var("v")])],
            ),
        ),
        typed_env_helper(
            "__elephc_opcache_env_float",
            TypeExpr::Float,
            e_cast(
                CastType::Float,
                e_call(
                    "trim",
                    vec![e_call("__elephc_ini_scan", vec![e_var("v")])],
                ),
            ),
        ),
        // `opcache.jit_prof_threshold` in the 8.2 profile ONLY: read as a double, REPORTED
        // truncated to an int.
        typed_env_helper(
            "__elephc_opcache_env_trunc",
            TypeExpr::Int,
            e_cast(
                CastType::Int,
                e_cast(
                    CastType::Float,
                    e_call(
                        "trim",
                        vec![e_call("__elephc_ini_scan", vec![e_var("v")])],
                    ),
                ),
            ),
        ),
        function("__elephc_opcache_env_pct")
            .param("u", TypeExpr::Str)
            .param("d", TypeExpr::Str)
            .param("def", TypeExpr::Float)
            .returns(TypeExpr::Float)
            .body(vec![
                s_assign(
                    "v",
                    e_call("__elephc_opcache_env", vec![e_var("u"), e_var("d")]),
                ),
                s_if(
                    e_binop(e_var("v"), BinOp::StrictEq, e_str("")),
                    vec![s_return(e_var("def"))],
                    vec![],
                    None,
                ),
                s_assign("s", e_call("__elephc_ini_scan", vec![e_var("v")])),
                s_if(
                    e_call("__elephc_ini_pct_ok", vec![e_var("s")]),
                    vec![s_return(e_call(
                        "__elephc_ini_pct_val",
                        vec![e_var("s")],
                    ))],
                    vec![],
                    None,
                ),
                s_return(e_var("def")),
            ])
            .build(),
        function("__elephc_opcache_env_str")
            .param("u", TypeExpr::Str)
            .param("d", TypeExpr::Str)
            .param("def", TypeExpr::Str)
            .returns(TypeExpr::Str)
            .body(vec![
                s_assign(
                    "v",
                    e_call("__elephc_opcache_env", vec![e_var("u"), e_var("d")]),
                ),
                s_if(
                    e_binop(e_var("v"), BinOp::StrictEq, e_str("")),
                    vec![s_return(e_var("def"))],
                    vec![],
                    None,
                ),
                s_assign("s", e_call("__elephc_ini_scan", vec![e_var("v")])),
                s_return(e_var("s")),
            ])
            .build(),
        // The RAW STRING surface feeding `ini_get()` / `ini_get_all()`. `$t` is the directive's
        // type code; only the percentage type can still refuse a value, and when it does BOTH
        // surfaces fall back to the compile-time default.
        function("__elephc_opcache_env_raw")
            .param("u", TypeExpr::Str)
            .param("d", TypeExpr::Str)
            .param("t", TypeExpr::Str)
            .param("def", TypeExpr::Str)
            .returns(TypeExpr::Str)
            .body(vec![
                s_assign(
                    "v",
                    e_call("__elephc_opcache_env", vec![e_var("u"), e_var("d")]),
                ),
                s_if(
                    e_binop(e_var("v"), BinOp::StrictEq, e_str("")),
                    vec![s_return(e_var("def"))],
                    vec![],
                    None,
                ),
                s_assign("s", e_call("__elephc_ini_scan", vec![e_var("v")])),
                s_if(
                    e_binop(e_var("t"), BinOp::StrictEq, e_str("p")),
                    vec![
                        s_if(
                            e_call("__elephc_ini_pct_ok", vec![e_var("s")]),
                            vec![s_return(e_var("s"))],
                            vec![],
                            None,
                        ),
                        s_return(e_var("def")),
                    ],
                    vec![],
                    None,
                ),
                s_return(e_var("s")),
            ])
            .build(),
    ]
}

/// One of `__elephc_ini_digit`'s three character-class arms: `if ($o >= lo && $o <= hi) { $d = $o - bias; }`.
fn digit_range(low: i64, high: i64, bias: i64) -> Stmt {
    s_if(
        e_binop(
            e_binop(e_var("o"), BinOp::GtEq, e_int(low)),
            BinOp::And,
            e_binop(e_var("o"), BinOp::LtEq, e_int(high)),
        ),
        vec![s_assign(
            "d",
            e_binop(e_var("o"), BinOp::Sub, e_int(bias)),
        )],
        vec![],
        None,
    )
}

/// One typed `ELEPHC_INI_*` helper: lookup, empty-means-unset floor, then `normalize`.
///
/// `__elephc_opcache_env_pct` and `_str` and `_raw` are spelled out separately because their
/// bodies branch after the scan rather than returning one expression.
fn typed_env_helper(name: &str, ty: TypeExpr, normalize: Expr) -> Stmt {
    function(name)
        .param("u", TypeExpr::Str)
        .param("d", TypeExpr::Str)
        .param("def", ty.clone())
        .returns(ty)
        .body(vec![
            s_assign(
                "v",
                e_call("__elephc_opcache_env", vec![e_var("u"), e_var("d")]),
            ),
            s_if(
                e_binop(e_var("v"), BinOp::StrictEq, e_str("")),
                vec![s_return(e_var("def"))],
                vec![],
                None,
            ),
            s_return(normalize),
        ])
        .build()
}

/// `__elephc_ini_quantity(string $v): int` — the PHP mirror of `zend_ini_parse_quantity`:
/// trim, optional sign, optional `0x`/`0b`/`0` radix prefix, digits, then a `k`/`m`/`g` suffix.
///
/// It accumulates in PHP integers where the Rust side carries a `u128` (so it cannot reproduce
/// `strtoul`'s ULONG_MAX-on-overflow result), and it does not carry the quantity DIAGNOSTICS —
/// `ini_override_warnings` emits those at compile time. Both are unreachable for a real
/// directive value and documented rather than modelled.
fn ini_quantity_decl() -> Stmt {
    /// `substr($v, <at>, 1)` — the one-character read this whole function is written in terms of.
    fn char_at(at: Expr) -> Expr {
        e_call("substr", vec![e_var("v"), at, e_int(1)])
    }

    function("__elephc_ini_quantity")
        .param("v", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("n", e_call("strlen", vec![e_var("v")])),
            s_if(
                e_binop(e_var("n"), BinOp::StrictEq, e_int(0)),
                vec![s_return(e_int(0))],
                vec![],
                None,
            ),
            // Trim to the half-open span [$s, $e).
            s_assign("s", e_int(0)),
            s_while(
                e_binop(
                    e_binop(e_var("s"), BinOp::Lt, e_var("n")),
                    BinOp::And,
                    e_call("__elephc_ini_isspace", vec![char_at(e_var("s"))]),
                ),
                vec![s_assign("s", e_binop(e_var("s"), BinOp::Add, e_int(1)))],
            ),
            s_assign("e", e_var("n")),
            s_while(
                e_binop(
                    e_binop(e_var("e"), BinOp::Gt, e_var("s")),
                    BinOp::And,
                    e_call(
                        "__elephc_ini_isspace",
                        vec![char_at(e_binop(e_var("e"), BinOp::Sub, e_int(1)))],
                    ),
                ),
                vec![s_assign("e", e_binop(e_var("e"), BinOp::Sub, e_int(1)))],
            ),
            s_if(
                e_binop(e_var("s"), BinOp::GtEq, e_var("e")),
                vec![s_return(e_int(0))],
                vec![],
                None,
            ),
            s_assign(
                "neg",
                e_binop(char_at(e_var("s")), BinOp::StrictEq, e_str("-")),
            ),
            s_assign("i", e_var("s")),
            s_assign("c", char_at(e_var("i"))),
            s_if(
                e_binop(
                    e_binop(e_var("c"), BinOp::StrictEq, e_str("-")),
                    BinOp::Or,
                    e_binop(e_var("c"), BinOp::StrictEq, e_str("+")),
                ),
                vec![s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1)))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("i"), BinOp::GtEq, e_var("e")),
                vec![s_return(e_int(0))],
                vec![],
                None,
            ),
            s_assign("o", e_call("ord", vec![char_at(e_var("i"))])),
            s_if(
                e_binop(
                    e_binop(e_var("o"), BinOp::Lt, e_int(48)),
                    BinOp::Or,
                    e_binop(e_var("o"), BinOp::Gt, e_int(57)),
                ),
                vec![s_return(e_int(0))],
                vec![],
                None,
            ),
            // A leading zero selects octal, and `0x`/`0b` override it.
            s_assign("radix", e_int(10)),
            s_if(
                e_binop(e_var("o"), BinOp::StrictEq, e_int(48)),
                vec![
                    s_assign("radix", e_int(8)),
                    s_if(
                        e_binop(
                            e_binop(e_var("i"), BinOp::Add, e_int(1)),
                            BinOp::Lt,
                            e_var("e"),
                        ),
                        vec![
                            s_assign(
                                "p",
                                e_call(
                                    "strtolower",
                                    vec![char_at(e_binop(
                                        e_var("i"),
                                        BinOp::Add,
                                        e_int(1),
                                    ))],
                                ),
                            ),
                            radix_prefix("x", 16),
                            radix_prefix("b", 2),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("i"), BinOp::GtEq, e_var("e")),
                vec![s_return(e_int(0))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_call(
                        "__elephc_ini_digit",
                        vec![char_at(e_var("i")), e_var("radix")],
                    ),
                    BinOp::Lt,
                    e_int(0),
                ),
                vec![s_return(e_int(0))],
                vec![],
                None,
            ),
            s_assign("acc", e_int(0)),
            s_while(
                e_binop(e_var("i"), BinOp::Lt, e_var("e")),
                vec![
                    s_assign(
                        "d",
                        e_call(
                            "__elephc_ini_digit",
                            vec![char_at(e_var("i")), e_var("radix")],
                        ),
                    ),
                    s_if(
                        e_binop(e_var("d"), BinOp::Lt, e_int(0)),
                        vec![s_break(1)],
                        vec![],
                        None,
                    ),
                    s_assign(
                        "acc",
                        e_binop(
                            e_binop(e_var("acc"), BinOp::Mul, e_var("radix")),
                            BinOp::Add,
                            e_var("d"),
                        ),
                    ),
                    s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                ],
            ),
            s_if(
                e_var("neg"),
                vec![s_assign("acc", e_neg(e_var("acc")))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("i"), BinOp::GtEq, e_var("e")),
                vec![s_return(e_var("acc"))],
                vec![],
                None,
            ),
            s_assign(
                "last",
                e_call(
                    "strtolower",
                    vec![char_at(e_binop(e_var("e"), BinOp::Sub, e_int(1)))],
                ),
            ),
            multiplier_suffix("k", 1024),
            multiplier_suffix("m", 1_048_576),
            multiplier_suffix("g", 1_073_741_824),
            s_return(e_var("acc")),
        ])
        .build()
}

/// One of `__elephc_ini_quantity`'s radix-prefix arms: `if ($p === '<letter>') { $radix = <radix>; $i = $i + 2; }`.
fn radix_prefix(letter: &str, radix: i64) -> Stmt {
    s_if(
        e_binop(e_var("p"), BinOp::StrictEq, e_str(letter)),
        vec![
            s_assign("radix", e_int(radix)),
            s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(2))),
        ],
        vec![],
        None,
    )
}

/// One of `__elephc_ini_quantity`'s trailing-multiplier arms: `if ($last === '<letter>') { return $acc * <factor>; }`.
fn multiplier_suffix(letter: &str, factor: i64) -> Stmt {
    s_if(
        e_binop(e_var("last"), BinOp::StrictEq, e_str(letter)),
        vec![s_return(e_binop(
            e_var("acc"),
            BinOp::Mul,
            e_int(factor),
        ))],
        vec![],
        None,
    )
}
