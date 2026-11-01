---
id: bug_fix-symfony-s-dynamic-routes-need-pcre-s-mark-verb-w-4f59ee14
type: bug_fix
title: "Symfony's dynamic routes need PCRE's MARK verb, which reaches $matches through four separate places"
description: "preg_match never populated $matches['MARK'], so CompiledUrlMatcher indexed dynamicRoutes[0] and every route with a placeholder died; the fix spans the pcre2 shim, a recipe revision, two assembly builders and the eval bridge"
created: 2026-09-17
verified_by: "A probe over '{^(?|/greet/([^/]++)(*:22)|...)}' matches php -n on the compiled AND interpreted paths; /greet/Bob now returns 'hello BOB' from the compiled --web server"
sources:
  - path: src/native_deps/recipes/pcre2_shim.c
    blob: 73ff9af8a528f02a8d4a94d11abda7abed034eb3
  - path: src/codegen_support/runtime/system/preg_match.rs
    blob: e9880b0917fa194669684eacc8baca36229406ba
  - path: crates/elephc-magician/src/regex_provider.rs
    blob: 3fb602617e5cc66524c83c414500abd6a8d1bded
  - path: crates/elephc-magician/src/interpreter/builtins/regex/captures.rs
    blob: 5edc1d8d59e8320f07c72f2ec6a93bbd2d4559bb
---

# Symfony's dynamic routes need PCRE's MARK verb, which reaches $matches through four separate places

## Fact

Symfony's dumped route matcher marks each alternative of its regexp with `(*:<offset>)` and picks
the branch with `$this->dynamicRoutes[(int) $matches['MARK']]`. elephc never produced that key, so
the index was `(int) null` = 0, `$dynamicRoutes[0]` was null, and `foreach (null as ...)` fatalled.

FINDING IT: the eval trace showed `array_reference_read key=Some(Int(0))` immediately after
`key=Some(String("MARK"))`, then `foreach_subject ... tag=8`. The `(int) 0` is the whole story; the
reported error (`eval callback dispatch failed` from the compiled callable invoker) named nothing
useful, because it is only the compiled side reporting that the interpreted call failed.

THE FIX SPANS FOUR PLACES, and missing any one leaves it silently absent:

1. `pcre2_shim.c` gained `elephc_pcre2_v1_last_mark`, wrapping `pcre2_get_mark()` over
   `handle->regex.re_match_data`.
2. THE RECIPE REVISION HAD TO MOVE WITH IT (6 -> 7 in `catalog.rs`, the dispatcher in
   `recipe.rs`, and that file's own "previous revisions are not dispatched" test). The catalog's
   cache key does not hash the shim source, so revision 6 would have relinked the STALE object
   with no error at all -- `recipe.rs` warns about exactly this and it is easy to walk into.
   `elephc native install` then reports "native lock is missing or stale" until it is rerun.
3. TWO assembly builders, not one: `__rt_preg_match_capture` is what `preg_match()` uses and
   `__rt_preg_match_row` is what `preg_match_all()` uses. Patching only the row builder changes
   nothing for `preg_match`. Both targets, both functions. A mark forces the HASH row for the same
   reason a declared group name does -- `MARK` is a string key.
4. The eval bridge carries its own provider table, so `__elephc_eval_register_regex_provider`
   grew a sixth callback and `eval_preg_capture_array` appends the key. Symfony's matcher runs
   INTERPRETED, so the compiled half alone fixes nothing visible.

STILL MISSING, measured against `php -n` 8.5 and deliberately left: the COMPILED
`PREG_OFFSET_CAPTURE` path (php puts MARK there as a bare string beside pair arrays), and the
INTERPRETED `preg_match_all` path (its rows are built from pre-collected captures, so a per-row
mark has to be recorded during the match loop). The compiled `preg_match_all` and the interpreted
`preg_match`/offset paths are all correct.

Found alongside it: the interpreter's `json_encode` chose its bracket from the STORAGE KIND, so a
hash keyed 0..n-1 -- the emptied array included -- printed `{}`/`{"0":..}` where php prints
`[]`/`[..]`. Symfony's `JsonResponse` shows it as `"post":{}` against php's `"post":[]`.

## Why

Nothing about the failure named preg_match: the visible symptom was a fatal inside CompiledUrlMatcher
