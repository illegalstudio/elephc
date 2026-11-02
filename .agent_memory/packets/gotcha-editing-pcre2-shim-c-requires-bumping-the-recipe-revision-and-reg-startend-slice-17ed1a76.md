---
type: "Gotcha"
title: "Editing pcre2_shim.c requires bumping the recipe revision, and REG_STARTEND slices the subject"
description: "Two things about the PCRE2 shim, learned the hard way in one sitting: 1. The native artifact cache key is package, version, recipe revision, source sha256, target and does NOT hash src/native deps/recipes/pcre2 shim.c ."
resource: "src/native_deps/recipes/pcre2_shim.c"
tags: ["session-learning", "pcre2", "native-deps", "regex", "symfony", "cache-key"]
timestamp: "2026-09-13T08:31:10.273Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:editing-pcre2-shim-c-requires-bumping-the-recipe-revision-and-reg-startend-slice"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/native_deps/recipes/pcre2_shim.c", "src/native_deps/catalog.rs", "src/native_deps/recipe.rs", "crates/elephc-magician/src/regex_provider.rs", "crates/elephc-magician/src/interpreter/builtins/regex/engine.rs"]
---

# Editing pcre2_shim.c requires bumping the recipe revision, and REG_STARTEND slices the subject

> Two things about the PCRE2 shim, learned the hard way in one sitting: 1. The native artifact cache key is package, ve…

Two things about the PCRE2 shim, learned the hard way in one sitting:

1. The native artifact cache key is `(package, version, recipe_revision, source_sha256, target)` and does NOT hash `src/native_deps/recipes/pcre2_shim.c`. Editing the shim without bumping `recipe_revision` in `src/native_deps/catalog.rs` (and the `("pcre2", N)` arm in `src/native_deps/recipe.rs`) silently relinks the STALE object -- `elephc native install` reports success and nothing changes. `recipe.rs`'s `previous_pcre2_recipe_revisions_are_not_dispatched` test is the guard; add the retired revision to it. Revision is now 6.

2. `pcre2_regexec`'s REG_STARTEND is implemented by ADVANCING the subject pointer (`pcre2_match(re, string + so, eo - so, 0, ...)`), so `^` under the `m` modifier matched at every continuation of a global match instead of at line starts. `preg_replace('/^./m', '    $0', $code)` indented every CHARACTER. Symfony's `CompiledUrlMatcherDumper` uses exactly that to indent its dump, so the routing cache it wrote was unparsable PHP. `elephc_pcre2_v1_exec` now calls `pcre2_match` directly with the whole subject plus a start offset (php's own preg does the same), and `crates/elephc-magician/src/regex_provider.rs`'s host-linked mirror was changed identically.

The AOT `preg_*` runtime (src/codegen_support/runtime/system/preg_replace.rs, preg_match_all.rs) still SLICES: it passes `current_pos` as the subject with flags 0 on every iteration, so `^`+`m` is still wrong for compiled code. That is unfixed.

A companion bug surfaced only once the anchor worked: `Regex::captures_iter_at` advanced the cursor to `end` after a match, which for a ZERO-WIDTH match found ahead of the cursor (`/^/m`) left the cursor on that same empty match and reported every line start twice. It now advances from the match's own end.
Evidence: scratchpad red31/a.php diffs `preg_replace('/^./m'|'/^/m'|'/.$/m')` and `preg_match_all('/^./m')` against `php -n` 8.5.10 in both AOT and eval frames; the eval half is now byte-identical, the AOT half still differs. Symfony's var/cache/dev/url_matching_routes.php went from unparsable to valid PHP.
Verified by: scratchpad/diff-red.sh on red31/a.php plus the regenerated Symfony routing cache

## Verification

scratchpad red31/a.php diffs `preg_replace('/^./m'|'/^/m'|'/.$/m')` and `preg_match_all('/^./m')` against `php -n` 8.5.10 in both AOT and eval frames; the eval half is now byte-identical, the AOT half still differs. Symfony's var/cache/dev/url_matching_routes.php went from unparsable to valid PHP.

# Citations

[1] explicit_capture (2026-09-13T08:31:10.273Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:editing-pcre2-shim-c-requires-bumping-the-recipe-revision-and-reg-startend-slice","title":"Editing pcre2_shim.c requires bumping the recipe revision, and REG_STARTEND slices the subject","summary":"Two things about the PCRE2 shim, learned the hard way in one sitting: 1. The native artifact cache key is package, version, recipe revision, source sha256, target and does NOT hash src/native deps/recipes/pcre2 shim.c .","body":"Two things about the PCRE2 shim, learned the hard way in one sitting:\n\n1. The native artifact cache key is `(package, version, recipe_revision, source_sha256, target)` and does NOT hash `src/native_deps/recipes/pcre2_shim.c`. Editing the shim without bumping `recipe_revision` in `src/native_deps/catalog.rs` (and the `(\"pcre2\", N)` arm in `src/native_deps/recipe.rs`) silently relinks the STALE object -- `elephc native install` reports success and nothing changes. `recipe.rs`'s `previous_pcre2_recipe_revisions_are_not_dispatched` test is the guard; add the retired revision to it. Revision is now 6.\n\n2. `pcre2_regexec`'s REG_STARTEND is implemented by ADVANCING the subject pointer (`pcre2_match(re, string + so, eo - so, 0, ...)`), so `^` under the `m` modifier matched at every continuation of a global match instead of at line starts. `preg_replace('/^./m', '    $0', $code)` indented every CHARACTER. Symfony's `CompiledUrlMatcherDumper` uses exactly that to indent its dump, so the routing cache it wrote was unparsable PHP. `elephc_pcre2_v1_exec` now calls `pcre2_match` directly with the whole subject plus a start offset (php's own preg does the same), and `crates/elephc-magician/src/regex_provider.rs`'s host-linked mirror was changed identically.\n\nThe AOT `preg_*` runtime (src/codegen_support/runtime/system/preg_replace.rs, preg_match_all.rs) still SLICES: it passes `current_pos` as the subject with flags 0 on every iteration, so `^`+`m` is still wrong for compiled code. That is unfixed.\n\nA companion bug surfaced only once the anchor worked: `Regex::captures_iter_at` advanced the cursor to `end` after a match, which for a ZERO-WIDTH match found ahead of the cursor (`/^/m`) left the cursor on that same empty match and reported every line start twice. It now advances from the match's own end.\nEvidence: scratchpad red31/a.php diffs `preg_replace('/^./m'|'/^/m'|'/.$/m')` and `preg_match_all('/^./m')` against `php -n` 8.5.10 in both AOT and eval frames; the eval half is now byte-identical, the AOT half still differs. Symfony's var/cache/dev/url_matching_routes.php went from unparsable to valid PHP.\nVerified by: scratchpad/diff-red.sh on red31/a.php plus the regenerated Symfony routing cache","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","pcre2","native-deps","regex","symfony","cache-key"],"paths":["src/native_deps/recipes/pcre2_shim.c","src/native_deps/catalog.rs","src/native_deps/recipe.rs","crates/elephc-magician/src/regex_provider.rs","crates/elephc-magician/src/interpreter/builtins/regex/engine.rs"],"stack":[],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-13T08:31:10.273Z"}],"context":{"fact":"Two things about the PCRE2 shim, learned the hard way in one sitting:","verification":"scratchpad red31/a.php diffs `preg_replace('/^./m'|'/^/m'|'/.$/m')` and `preg_match_all('/^./m')` against `php -n` 8.5.10 in both AOT and eval frames; the eval half is now byte-identical, the AOT half still differs. Symfony's var/cache/dev/url_matching_routes.php went from unparsable to valid PHP."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-13T08:31:10.273Z","path_fingerprints":[{"path":"src/native_deps/recipes/pcre2_shim.c","sha256":"e5e125a2a34ce3bb1ad9b02d0450980f04a6ae404a111a19f7de4286f4b09355","size":11250},{"path":"src/native_deps/catalog.rs","sha256":"d4039f32dcc7ee5daa0be135b7a947e852f5c265d97f79065476cf47788b8746","size":8130},{"path":"src/native_deps/recipe.rs","sha256":"5c0e2043dd77edec5861ecff8d198cd83766921895c5c85e0b0531e11a9eac17","size":4346},{"path":"crates/elephc-magician/src/regex_provider.rs","sha256":"4e15fb9416b19067dff8cbf189cd248876b04dd322232320414de28ae7ff51c2","size":20580},{"path":"crates/elephc-magician/src/interpreter/builtins/regex/engine.rs","sha256":"b2c2fd6e1057f35bac6a4bc27fef91433943a0a17c1533e3e5739f7825b12706","size":11222}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":555},"created_at":"2026-09-13T08:31:10.273Z","updated_at":"2026-09-13T08:31:10.273Z","author_branch":"reconcile/dirname-symfony"}
```

