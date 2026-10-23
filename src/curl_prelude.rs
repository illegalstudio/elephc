//! Purpose:
//! PHP's `ext/curl` surface in elephc-PHP: the `CurlHandle` class plus the easy-handle
//! functions (`curl_init` / `curl_setopt` / `curl_setopt_array` / `curl_exec` /
//! `curl_getinfo` / `curl_close` / `curl_errno` / `curl_error` / `curl_version` / …), and
//! the `CurlMultiHandle` class plus the whole `curl_multi_*` family, implemented on top of
//! the internal `__elephc_curl_*` builtins. PHP 8 models a session as an OBJECT
//! (`CurlHandle`, `CurlMultiHandle`), not a resource, and this prelude is what makes
//! `curl_init()`/`curl_multi_init()` return real objects that `is_object`, `get_class`,
//! `instanceof`, and `var_dump` all agree about.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`,
//!   after the hash prelude and before name resolution.
//!
//! Key details:
//! - WHY A PRELUDE AND NOT A NATIVE CLASS. Every curl `RuntimeFnId` declares
//!   `BuiltinRequirement::Bridge("elephc_curl")` (`crate::ir::runtime_fn`), and the
//!   bridge in turn requires the managed native `curl` package
//!   (`crate::pipeline::backend`), so the surface is bridge-gated: a program that never
//!   uses curl must neither declare `CurlHandle` nor link `-lelephc_curl` nor need
//!   `libcurl.a`. Injecting only on demand (pay-for-use) preserves that, and the
//!   whole feature then compiles through the ordinary class/function pipeline.
//! - WHY THE BUILTINS ARE INTERNAL. A prelude function cannot shadow a builtin of the
//!   same name (`Cannot redeclare built-in function`), so the raw work lives in
//!   `internal: true` `__elephc_curl_*` builtins (`crate::builtins::curl`) and the PHP
//!   names are these wrappers. `function_exists('curl_init')` is therefore true exactly
//!   when the prelude is injected.
//! - OWNERSHIP: the object holds the Mixed handle cell in `$__elephc_handle` and adds NO
//!   retain/release of its own. That cell owns the native libcurl easy handle: the
//!   standard object-free path (`__rt_heap_free` → `object_free_deep`) releases property
//!   storage, which reaches `__rt_mixed_free_deep` and thence `__rt_curl_easy_free`
//!   (resource kind 6). There is deliberately no `__destruct`: adding one would
//!   introduce a second free path.
//! - `curl_close()` IS A NO-OP, exactly as in PHP 8 — the handle stays usable until the
//!   object is destroyed. It is declared so `function_exists('curl_close')` is true and
//!   so existing PHP code that calls it keeps working.
//! - EVERY wrapper BINDS `$handle->__elephc_handle` TO A LOCAL (`$raw`) BEFORE CALLING.
//!   This is not style. Passing a `mixed` object property *inline* as a call argument
//!   trips a pre-existing codegen leak (one heap block per call), documented in full in
//!   `crate::hash_prelude`; binding to a local sidesteps it. The same rule applies to
//!   every wrapper added by later curl tasks.
//! - `CURLOPT_RETURNTRANSFER` (19913) is mirrored into `$__elephc_return_transfer`
//!   because `curl_exec()`'s RETURN SHAPE depends on it (`string` when capturing,
//!   `true` otherwise) and the bridge's C ABI exposes no way to ask a handle whether it is
//!   capturing. The option is still forwarded to the bridge, which owns the actual
//!   capture behaviour (`crates/elephc-curl/src/php_layer.rs`); the property only
//!   selects between `curl_exec()`'s two success shapes.
//! - `__serialize()` THROWS rather than returning a reduced array, matching php-src:
//!   `CurlHandle` is not serializable there either
//!   (`Exception: Serialization of 'CurlHandle' is not allowed`), and elephc could not
//!   reproduce a live libcurl handle from a serialized blob in any case.
//! - `curl_version()` decodes the bridge's JSON blob through the ordinary `json_decode`
//!   builtin instead of a bespoke array builder, so the array reflects the libcurl that
//!   is actually linked AT RUN TIME rather than anything baked in at compile time.
//! - `curl_setopt()` CLASSIFIES THE OPTION NUMBER BEFORE FORWARDING, because
//!   `curl_easy_setopt` reads its variadic argument according to the option's numeric
//!   RANGE — forwarding a PHP value into a pointer-typed range is a wild pointer, not
//!   merely a wrong answer. The classification comes from the bridge's frozen option
//!   table (`crates/elephc-curl/src/options.rs`) through
//!   `__elephc_curl_option_kind()`; see the comment above the function for the kind
//!   codes and the libcurl source that fixes the ranges. An option php-src does not
//!   recognize at all raises php-src's own `ValueError`; one it recognizes that this
//!   build cannot carry answers `false` with a PHP warning, never an inert `true`.
//! - `curl_getinfo()` DISPATCHES ON THE `CURLINFO_*` TYPE MASK, the read-side mirror of
//!   `curl_setopt()`'s option table and php-src's own structure: string/long/double/slist/
//!   off_t each read through their own typed entry point, three options are special-cased
//!   before the mask (`CURLINFO_PRIVATE`, `CURLINFO_CERTINFO`, `CURLINFO_HEADER_OUT`), and
//!   anything else answers `false` — never a fabricated value. The no-`$option` form
//!   returns PHP's documented associative array, built as JSON by the bridge and decoded
//!   with the ordinary `json_decode` builtin, the same route `curl_version()` takes.
//! - FIVE SIGNATURES DIVERGE FROM php-src, all forced by the same checker limitation
//!   (a `T|false` union is accepted nowhere a `T` is expected, and is not narrowed by a
//!   `=== false` guard) and all documented at their declaration below: `curl_init()` and
//!   `curl_copy_handle()` return a plain `CurlHandle` and THROW instead of returning
//!   `CurlHandle|false`; `curl_escape()`/`curl_unescape()` return a plain `string` and
//!   throw instead of `string|false`; `curl_version()` leaves its return type undeclared
//!   instead of `array|false`. `curl_strerror()` returns `string` rather than `?string`,
//!   which is not a limitation but a value that is never null. Runtime behaviour is
//!   otherwise PHP's. `docs/php/curl.md`'s "Differences from PHP" section is where these reach users.
//! - THE MULTI INTERFACE ADDS FOUR MORE SIGNATURE DIVERGENCES:
//!   `curl_multi_info_read()` leaves its
//!   return type undeclared instead of `array|false` (declaring the union makes a
//!   `$info === false` guard answer TRUE for a real array once the same variable is later
//!   indexed — measured, the exact shape `curl_version()` documents); and
//!   `curl_multi_add_handle()`/`curl_multi_remove_handle()`/`curl_multi_getcontent()` take
//!   `mixed $handle` instead of `CurlHandle $handle`, with a runtime `instanceof` guard.
//!   This is now a CHECKER limitation only: call lowering correctly unboxes a Mixed-sourced
//!   object after `instanceof` narrowing, but the checker rejects the canonical unguarded
//!   `$info['handle']` / list-element call before it can reach that lowering. Keeping the
//!   public parameter `mixed` lets the wrapper perform the runtime check PHP performs.
//! - THE SHARE INTERFACE ADDS NO NEW SIGNATURE DIVERGENCE of those families. It DOES add one new `curl_setopt()`
//!   `$kind` (`7`, `KIND_SHARE`): `CURLOPT_SHARE`'s value is the ONE object read out of
//!   `curl_setopt()`'s `mixed $value` in this whole file, handled before the scalar-type
//!   guard (see the `if ($kind === 7)` branch below) and requiring an `elephc_curl_easy_
//!   set_share()` bridge call rather than any of the three ordinary setters.
//! - THE SHARE LIFETIME QUESTION ("does freeing `$sh` before `$ch` corrupt anything?") is
//!   closed at the BRIDGE, not here, and the real answer is more mundane than a
//!   use-after-free: `crates/elephc-curl/src/share.rs`'s module doc carries the full
//!   argument. libcurl 8.21.0 REFCOUNTS a share (`CURLOPT_SHARE` increments it, an easy
//!   handle's own close decrements it), so `curl_share_cleanup()` while an easy handle
//!   still references it does not corrupt anything — it FAILS (`CURLSHE_IN_USE`) and frees
//!   nothing, which is a silent PERMANENT LEAK if that failure is ignored, not a crash.
//!   `curl_setopt($ch, CURLOPT_SHARE, $sh)` needs no PHP-level reference from `$ch` to
//!   `$sh` (the way `CurlMultiHandle::__elephc_attach()` takes one for an added easy
//!   handle) because the BRIDGE keeps its own count of attached easy handles and DEFERS the
//!   real `curl_share_cleanup()` call until that count reaches zero — mirroring, at the
//!   bridge level, the real Zend GC reference php-src itself relies on to avoid the same
//!   leak. `CurlShareHandle`'s Mixed cell teardown therefore never forcibly touches a live
//!   easy handle's `CURLOPT_SHARE` at all; this holds regardless of `unset()` order.
//! - `curl_share_init_persistent()` (PHP 8.5) IS PROCESS-LIFETIME, mirroring
//!   `curl_multi_get_handles()`'s PHP-8.5-only version gate but adding its own:
//!   the underlying native share, once created, is NEVER freed by
//!   `__elephc_curl_share_free()` — elephc has no PHP-FPM-worker-restart boundary to key a
//!   shorter lifetime off, so "process lifetime" is the honest answer. See that function's
//!   own comment below.
//! - `CURLFile`/`CURLStringFile` ARE PLAIN PHP DATA CLASSES, UNLIKE EVERY OTHER
//!   CLASS IN THIS FILE: neither wraps a native handle, so neither has `$__elephc_handle`,
//!   a private constructor, or a `__elephc_wrap` factory — both are ordinary, user-
//!   constructible classes with public properties, matching php-src's own shape
//!   (`ext/curl/curl_file.stub.php`). `curl_setopt(..., CURLOPT_POSTFIELDS, $array)`'s
//!   ARRAY FORM POSTS REAL `multipart/form-data`: `__elephc_curl_build_multipart()` below walks the
//!   array through `crates/elephc-curl/src/mime.rs`'s builder ABI field by field. A NESTED
//!   ARRAY VALUE FLATTENS ONE LEVEL — PHP PARITY, measured against a real `ext/curl`: one
//!   part per inner element, all sharing the outer key as their field name, matching
//!   php-src's own `build_mime_structure_from_hash` exactly. Its own comment records the
//!   two divergences that remain: a DOUBLY-nested value (an inner element that is itself
//!   an array) or an object inside a nested array is refused with a `\TypeError` instead
//!   of reproducing php-src's own recursion limit — the array case is measured against a
//!   real `ext/curl`, the object case is inferred from the same code path rather than
//!   separately measured — and the one FILE-NOT-FOUND divergence (elephc fails at
//!   `curl_setopt()` time via `curl_mime_filedata`'s own eager validation; php-src fails
//!   later, at `curl_exec()`, via a custom read callback this build does not have).

pub(crate) mod build;
mod detect;
#[cfg(test)]
mod oracle;
#[cfg(test)]
pub(crate) use oracle::{prelude_source_for_version, CURL_PRELUDE_SRC};

/// The elephc-PHP curl prelude: the `CurlHandle` class and the easy-handle functions
/// that produce and consume it.
///
/// TEST-ONLY MIGRATION ORACLE. The shipped declarations are built in Rust by
/// [`build::curl_declarations`]; this text is retained so the oracle can prove the built
/// AST matches the PHP it replaced, node by node, across every PHP-version profile.

/// Injects the curl prelude when the program references the `ext/curl` surface, leaving
/// every other program untouched, for the DEFAULT PHP compatibility version.
///
/// `force` comes from an explicit opt-in (`--with-curl`, and the codegen harness);
/// otherwise the decision is `detect::program_uses_curl`. The prelude carries only
/// declarations, so prepending it is order-independent — PHP hoists them.
pub fn inject_if_used(
    program: crate::parser::ast::Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> crate::parser::ast::Program {
    inject_if_used_for_version(
        program,
        force,
        crate::php_version::PhpVersion::default(),
        inventory,
    )
}

/// Injects the curl prelude generated for an explicit PHP compatibility version.
///
/// The curl surface is not the same in every supported PHP: `curl_multi_get_handles()`
/// exists only from 8.5, so compiling with `--php-version 8.4` must
/// leave it undeclared and let the call fail as "undefined function", the way that runtime
/// would. The version-specific parts of the source are fenced with
/// `// -- elephc PHP >= <version> … begin/end --` markers and removed by
/// [`prelude_source_for_version`], the same mechanism `crate::pdo_prelude` uses.
///
/// Runs under `crate::compiler_stack::with_compiler_stack` (issue #1149): its usage scan walks the
/// whole program, so an embedder calling it without the CLI driver's wrapper gets the same stack
/// budget.
pub fn inject_if_used_for_version(
    program: crate::parser::ast::Program,
    force: bool,
    php_version: crate::php_version::PhpVersion,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> crate::parser::ast::Program {
    crate::compiler_stack::with_compiler_stack(|| {
        inject_if_used_for_version_on_compiler_stack(program, force, php_version, inventory)
    })
}

/// The body of [`inject_if_used_for_version`], run on the stack `with_compiler_stack` sized.
fn inject_if_used_for_version_on_compiler_stack(
    program: crate::parser::ast::Program,
    force: bool,
    php_version: crate::php_version::PhpVersion,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> crate::parser::ast::Program {
    if !force && !detect::program_uses_curl(&program) {
        return program;
    }
    // BUILT IN RUST, not parsed. The declarations are constructed directly by
    // `crate::curl_prelude::build::curl_declarations`, the same way every other stdlib
    // prelude works; `CURL_PRELUDE_SRC` and `prelude_source_for_version` remain only as
    // the migration oracle that proves the built AST matches the PHP it replaced.
    let mut combined = build::curl_declarations(php_version);
    // Declaration reachability prunes what nothing reaches, so the surface has to be
    // declared to it: this is what lets `--with-curl` keep the whole thing (through
    // `PruneOptions::forced_groups`) for a program whose only route to curl is one the
    // compiler cannot see. Nothing here is registered with `record_internal_callable_method`
    // — see `reachability_tests` below for the measurement that says why not.
    inventory.record_program("curl", &combined);
    combined.extend(program);
    combined
}


#[cfg(test)]
mod version_tests {
    use super::*;
    use crate::php_version::PhpVersion;

    /// 8.5 (elephc's default) declares `curl_multi_get_handles()`; every earlier profile
    /// must not, because those runtimes do not have the function at all.
    #[test]
    fn curl_multi_get_handles_is_php_85_only() {
        for version in PhpVersion::ALL {
            let source = prelude_source_for_version(version);
            let declared = source.contains("function curl_multi_get_handles(");
            assert_eq!(
                declared,
                version >= PhpVersion::Php85,
                "curl_multi_get_handles must be declared exactly for PHP >= 8.5, not {version}"
            );
        }
    }

    /// Stripping the 8.5 block must not take any of the version-independent multi surface
    /// with it — the failure mode of a mis-placed end marker.
    #[test]
    fn every_version_keeps_the_version_independent_multi_surface() {
        for version in PhpVersion::ALL {
            let source = prelude_source_for_version(version);
            for declaration in [
                "final class CurlMultiHandle {",
                "function curl_multi_init(): CurlMultiHandle {",
                "function curl_multi_add_handle(",
                "function curl_multi_exec(",
                "function curl_multi_info_read(",
                "function curl_multi_getcontent(",
                "function curl_multi_strerror(",
            ] {
                assert!(
                    source.contains(declaration),
                    "{version} must keep {declaration}"
                );
            }
        }
    }

    /// `curl_share_init_persistent()`/`CurlSharePersistentHandle` are PHP 8.5 ONLY, the
    /// same PHP-version-profile gate `curl_multi_get_handles()` has — every earlier
    /// profile must not declare either.
    #[test]
    fn curl_share_init_persistent_is_php_85_only() {
        for version in PhpVersion::ALL {
            let source = prelude_source_for_version(version);
            for declaration in [
                "function curl_share_init_persistent(",
                "final class CurlSharePersistentHandle {",
            ] {
                assert_eq!(
                    source.contains(declaration),
                    version >= PhpVersion::Php85,
                    "{declaration} must be declared exactly for PHP >= 8.5, not {version}"
                );
            }
        }
    }

    /// Stripping EITHER 8.5 block must not take the version-independent SHARE surface with
    /// it: `CurlShareHandle` and its functions are PHP 8.0+ and must survive every profile,
    /// unlike `CurlSharePersistentHandle` above.
    #[test]
    fn every_version_keeps_the_version_independent_share_surface() {
        for version in PhpVersion::ALL {
            let source = prelude_source_for_version(version);
            for declaration in [
                "final class CurlShareHandle {",
                "function curl_share_init(): CurlShareHandle {",
                "function curl_share_setopt(",
                "function curl_share_close(",
                "function curl_share_errno(",
                "function curl_share_strerror(",
            ] {
                assert!(
                    source.contains(declaration),
                    "{version} must keep {declaration}"
                );
            }
        }
    }

    /// The gated source still tokenizes and parses on every profile: a stripped block that
    /// left an unbalanced brace would otherwise only fail when a user compiled for 8.4.
    #[test]
    fn every_version_parses() {
        for version in PhpVersion::ALL {
            let source = prelude_source_for_version(version);
            let tokens = crate::lexer::tokenize(source.as_ref())
                .unwrap_or_else(|e| panic!("{version} curl prelude must tokenize: {e:?}"));
            crate::parser::parse_internal(&tokens)
                .unwrap_or_else(|e| panic!("{version} curl prelude must parse: {e:?}"));
        }
    }
}

/// THE SURFACE AUDIT: every PHP-visible name in the frozen
/// `scripts/docs/curl_surface.json` has a home, and naming it is enough to pull the
/// prelude in.
///
/// The three halves of "does elephc implement `ext/curl`?" live in three different
/// places, and each needs its own ratchet against the same frozen file:
///
/// - FUNCTIONS and CLASSES — here. Declared by the prelude, and reachable by
///   `detect::program_uses_curl` so the declaration is actually injected.
/// - CONSTANTS — the shared constant catalog (`elephc_builtin_contract` `catalog_constants_curl`)
///   (`curl_constants_match_frozen_surface` in that crate's `symbol_registry`, both
///   directions, 689 names); Magician reads the same catalog, there is no eval fork.
/// - `CURLOPT_*` BEHAVIOUR — `elephc_curl::tests`
///   (`every_frozen_curlopt_is_classified` proves none is silently unknown;
///   `option_table_matches_the_frozen_surface` proves none is silently misclassified,
///   with the deliberate rejections listed by name).
///
/// This module deliberately re-reads the JSON instead of hard-coding a second list:
/// a name added to the frozen surface without a home must FAIL here, which is the
/// only thing that keeps "complete `ext/curl` surface" an auditable claim rather
/// than a prose one.
#[cfg(test)]
mod surface_audit_tests {
    use super::*;
    use crate::php_version::PhpVersion;
    use std::path::Path;

    /// The names the frozen surface records as PHP 8.5 additions.
    /// Their `sources` entry is the plan/docs marker rather than a probed 8.4 binary.
    const PHP_85_SOURCE_MARKER: &str = "php-8.5 (plan/docs)";

    /// Re-parses the frozen surface at test time — the same source of truth the
    /// constant tables and the bridge's option table are generated from.
    fn frozen_surface() -> serde_json::Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/docs/curl_surface.json");
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        serde_json::from_str(&raw).expect("curl_surface.json must be valid JSON")
    }

    /// Returns whether the frozen entry for a name is marked PHP 8.5-only.
    fn is_php_85_only(entry: &serde_json::Value) -> bool {
        if let Some(sources) = entry.get("sources").and_then(|v| v.as_array()) {
            return sources
                .iter()
                .all(|source| source.as_str() == Some(PHP_85_SOURCE_MARKER));
        }
        entry.get("source_note").and_then(|v| v.as_str()) == Some(PHP_85_SOURCE_MARKER)
    }

    /// EVERY function in the frozen surface is declared by the prelude for the PHP
    /// version that has it — and the 8.5-only ones are declared for 8.5 ONLY, so an
    /// `--php-version 8.4` build fails them as "undefined function" the way that
    /// runtime would.
    #[test]
    fn every_frozen_function_is_declared_for_its_php_versions() {
        let surface = frozen_surface();
        let functions = surface["functions"]
            .as_object()
            .expect("frozen surface must carry a functions map");
        assert_eq!(functions.len(), 35, "the frozen ext/curl function count changed");

        for (name, entry) in functions {
            let php_85_only = is_php_85_only(entry);
            let declaration = format!("function {name}(");
            for version in PhpVersion::ALL {
                let source = prelude_source_for_version(version);
                let declared = source.contains(&declaration);
                let expected = !php_85_only || version >= PhpVersion::Php85;
                assert_eq!(
                    declared, expected,
                    "{name}() must be declared exactly when expected on {version} \
                     (php_85_only={php_85_only}); the frozen surface lists it, so it \
                     needs a prelude wrapper or an explicit version gate"
                );
            }
        }
    }

    /// EVERY class in the frozen surface is declared by the prelude, under the same
    /// 8.5 gate. `CURLFile`/`CURLStringFile` are user-constructible; the four handle
    /// classes are `final` and minted only by the runtime.
    #[test]
    fn every_frozen_class_is_declared_for_its_php_versions() {
        let surface = frozen_surface();
        let classes = surface["classes"]
            .as_object()
            .expect("frozen surface must carry a classes map");
        assert_eq!(classes.len(), 6, "the frozen ext/curl class count changed");

        for (name, entry) in classes {
            let php_85_only = is_php_85_only(entry);
            for version in PhpVersion::ALL {
                let source = prelude_source_for_version(version);
                let declared = source.contains(&format!("class {name} {{"))
                    || source.contains(&format!("class {name}\n"));
                let expected = !php_85_only || version >= PhpVersion::Php85;
                assert_eq!(
                    declared, expected,
                    "class {name} must be declared exactly when expected on {version} \
                     (php_85_only={php_85_only})"
                );
            }
        }
    }

    /// A DECLARATION IS USELESS IF DETECTION NEVER INJECTS IT. Naming any frozen
    /// function must make `detect::program_uses_curl` true, or a program calling it
    /// would compile as "undefined function" while the prelude sat unused (locked
    /// decision 4 makes injection demand-driven, so this is the failure mode).
    #[test]
    fn naming_any_frozen_function_triggers_injection() {
        let surface = frozen_surface();
        let functions = surface["functions"]
            .as_object()
            .expect("frozen surface must carry a functions map");
        for name in functions.keys() {
            let program = format!("<?php {name}();");
            assert!(
                program_triggers_curl(&program),
                "calling {name}() must trigger curl prelude injection, but detection \
                 did not recognize it (see src/curl_prelude/detect.rs)"
            );
        }
    }

    /// The same gate for the six classes, in the two positions that matter most: a
    /// `new` expression and an `instanceof` test. `CurlHandle` is not constructible at
    /// run time, but detection is purely syntactic and must fire either way.
    #[test]
    fn naming_any_frozen_class_triggers_injection() {
        let surface = frozen_surface();
        let classes = surface["classes"]
            .as_object()
            .expect("frozen surface must carry a classes map");
        for name in classes.keys() {
            for program in [
                format!("<?php $x = new {name}();"),
                format!("<?php $x = null; $y = $x instanceof {name};"),
                format!("<?php function f({name} $h): void {{ $h->x = 1; }}"),
            ] {
                assert!(
                    program_triggers_curl(&program),
                    "{program} must trigger curl prelude injection, but detection did \
                     not recognize class {name}"
                );
            }
        }
    }

    /// Parses a PHP snippet and asks the real detector whether it uses curl.
    fn program_triggers_curl(source: &str) -> bool {
        let tokens = crate::lexer::tokenize(source)
            .unwrap_or_else(|e| panic!("audit snippet must tokenize ({source}): {e:?}"));
        let program = crate::parser::parse_internal(&tokens)
            .unwrap_or_else(|e| panic!("audit snippet must parse ({source}): {e:?}"));
        detect::program_uses_curl(&program)
    }

    /// The audit's own negative control: a curl-free program must NOT pull the prelude
    /// in. Without this, a detector that always answered `true` would pass every test
    /// above while breaking the pay-for-use guarantee for the whole language.
    #[test]
    fn a_curl_free_program_does_not_trigger_injection() {
        for source in [
            "<?php echo 1;",
            "<?php function curl_helper($x) { return $x; } curl_helper(1);",
            "<?php $c = new Curler(); $c->run();",
        ] {
            assert!(
                !program_triggers_curl(source),
                "{source} must NOT trigger curl prelude injection"
            );
        }
    }
}

/// How the injected curl surface meets declaration reachability.
///
/// Declaration pruning runs over EVERY program and the curl prelude is the largest thing it
/// can be handed, so what it may and may not delete here is worth pinning rather than
/// inferring. Two facts, each measured rather than assumed:
///
/// 1. `--with-curl` needs the prelude's inventory group in `PruneOptions::forced_groups`, or
///    the flag injects a surface the pruner deletes again on the next pass.
/// 2. The prelude's dynamic-dispatch sites hand off a USER-SUPPLIED callable, so their
///    dynamic hazards are load-bearing and must NOT be cleared the way
///    `PreludeInventory::record_internal_callable_method` clears PDO's. NO curl declaration
///    is registered, and what keeps that answer honest is
///    `crate::optimize::reachability::tests::curl_prelude_dispatches_only_user_callables`,
///    which enumerates the hazard sites — it lives there rather than here because the
///    per-declaration scan it reads is private to the reachability module.
#[cfg(test)]
mod reachability_tests {
    use std::collections::HashSet;

    use crate::optimize::reachability::{PreludeInventory, PruneOptions};
    use crate::parser::ast::{Program, Stmt, StmtKind};

    /// Runs the real frontend over `source` with the curl prelude injected.
    fn injected(source: &str, force: bool) -> (Program, PreludeInventory, crate::types::CheckResult) {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        let program = crate::parser::parse(&tokens).expect("fixture must parse");
        let program = crate::autoload::collect_aliases(program).expect("alias collection failed");
        let mut inventory = PreludeInventory::new();
        let program = super::inject_if_used(program, force, &mut inventory);
        // The prelude's OpenBaseDir guard reads ini_get; the ini surface ships with the
        // injection the production pipeline adds for a program that references it.
        let program = crate::opcache_prelude::inject_ini_wrappers(
            program,
            crate::php_version::PhpVersion::default(),
            &[],
            &mut inventory,
        );
        let program = crate::name_resolver::resolve(program).expect("fixture must resolve");
        let program = crate::func_args::desugar(program).expect("fixture must desugar");
        let program = crate::optimize::fold_constants(program);
        let check = crate::types::check(&program).expect("fixture must type check");
        (program, inventory, check)
    }

    /// Prunes one injected fixture, optionally forcing the `curl` inventory group.
    fn prune(source: &str, force: bool, forced_group: bool) -> Program {
        let (program, inventory, mut check) = injected(source, force);
        let forced: HashSet<String> = if forced_group {
            ["curl".to_string()].into_iter().collect()
        } else {
            HashSet::new()
        };
        let exported = HashSet::new();
        crate::optimize::prune_unreachable_declarations(
            program,
            &mut check,
            PruneOptions {
                inventory: &inventory,
                forced_groups: &forced,
                exported_functions: &exported,
                eval_forced: false,
            },
        )
    }

    /// Returns whether a top-level function declaration survives.
    fn has_function(program: &[Stmt], name: &str) -> bool {
        program.iter().any(|statement| {
            matches!(&statement.kind, StmtKind::FunctionDecl { name: declared, .. }
                if declared.eq_ignore_ascii_case(name))
        })
    }

    /// Returns whether a top-level class declaration survives.
    fn has_class(program: &[Stmt], name: &str) -> bool {
        program.iter().any(|statement| {
            matches!(&statement.kind, StmtKind::ClassDecl { name: declared, .. }
                if declared.eq_ignore_ascii_case(name))
        })
    }

    /// Verifies `--with-curl` keeps the whole injected surface, and that it takes the forced
    /// inventory group to do it.
    ///
    /// The fixture NEVER MENTIONS CURL — that is the case `--with-curl` exists for, a program
    /// whose only route to the surface is one the compiler cannot see (an `eval()` body, a
    /// computed function name). Injection alone is not enough for it: with no executable edge
    /// and no forced group, reachability correctly concludes nothing reaches `curl_init()` and
    /// deletes the surface the flag just asked for. The negative half is the point of the
    /// test, not decoration.
    #[test]
    fn forcing_the_curl_group_keeps_the_whole_surface() {
        let source = "<?php function helper(): int { return 1; } echo helper();";

        let forced = prune(source, true, true);
        assert!(has_function(&forced, "curl_init"), "--with-curl must keep curl_init()");
        assert!(
            has_function(&forced, "curl_multi_init"),
            "--with-curl must keep the multi surface"
        );
        assert!(has_class(&forced, "CurlHandle"), "--with-curl must keep CurlHandle");
        assert!(has_class(&forced, "CURLFile"), "--with-curl must keep CURLFile");

        let unforced = prune(source, true, false);
        assert!(
            !has_function(&unforced, "curl_init") && !has_class(&unforced, "CurlHandle"),
            "without the forced group the injected surface is pruned away again — which is \
             why `pipeline::compile` lists `curl` in `forced_groups`"
        );
    }

    /// Verifies a callback named only by a STRING through `curl_setopt()` survives pruning.
    ///
    /// THIS IS WHY NO CURL DECLARATION IS REGISTERED WITH
    /// `PreludeInventory::record_internal_callable_method`. PDO registers four methods whose
    /// generic callable syntax only ever invokes closure descriptors the prelude itself
    /// created; clearing their dynamic hazards is safe because no user declaration can be on
    /// the other end. Curl's two dynamic sites are the opposite: `curl_setopt()` asks
    /// `is_callable($value)` about the USER'S value, and `__elephc_curl_sync_read_slot`'s
    /// dispatcher `call_user_func($user, …)`s the user's read callback. The user writes the
    /// name as a bare string into a `mixed $value` parameter, and nothing connects that string
    /// to the declaration — so the prelude's dynamic-function hazard is the ONLY thing keeping
    /// `my_writer` alive, and registering `curl_setopt` as internal-callable would silently
    /// delete a callback the program still calls. Verified by removing the `curl_setopt()`
    /// line: `my_writer` is then pruned and this assertion fails.
    #[test]
    fn a_string_named_curl_callback_survives_pruning() {
        let pruned = prune(
            "<?php function my_writer($ch, string $data): int { return 1; } \
             $ch = curl_init('https://example.com'); \
             curl_setopt($ch, CURLOPT_WRITEFUNCTION, 'my_writer'); \
             curl_exec($ch);",
            false,
            false,
        );
        assert!(
            has_function(&pruned, "my_writer"),
            "a CURLOPT_WRITEFUNCTION callback named by string must not be pruned"
        );
    }
}
