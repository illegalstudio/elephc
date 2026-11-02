---
id: gotcha-preg-failure-results-come-from-the-shim-s-last-e-bedabb4d
type: gotcha
title: "preg failure results come from the shim's last-error code, and a shim change needs a recipe bump in three places"
description: "A /u literal pattern types preg_match as int|false and preg_replace as ?string; codegen and eval read elephc_pcre2_v1_last_error after the call. Editing pcre2_shim.c requires bumping the recipe revision or the stale cached object links silently"
created: 2026-09-26
sources:
  - path: src/native_deps/recipes/pcre2_shim.c
    blob: 245ac1964cdc2663fd54cccd613b340599cda938
  - path: src/native_deps/catalog.rs
    blob: 0bf3fe905d2355ae4740cdd1ed838d0ab5efd2d0
  - path: src/native_deps/recipe.rs
    blob: 229ab45c7e4b0f7de30036ae7b23a4c7093714f2
  - path: src/builtins/system/preg_match.rs
    blob: a1954b7c1d4373252ee975a7389641d66ec863e8
  - path: src/codegen/lower_inst/builtins/regex.rs
    blob: 1e55e670066749508876e1c1608a0e12f37c24a0
  - path: crates/elephc-magician/src/regex_provider.rs
    blob: 4f4f43cc9cd15a899b9381cbb12ac61de225b010
---

# preg failure results come from the shim's last-error code, and a shim change needs a recipe bump in three places

## Fact

Semantics: under /u a subject that is not valid UTF-8 makes PCRE2 fail; php reports it in the
result (preg_match false, preg_replace null, preg_last_error() 4). Before this, the shim mapped
every rc<0 to REG_BADPAT and the runtime read it as "no more matches": the subject came back
unchanged.

Design: the shim keeps `elephc_pcre2_v1_error` (reset by compile, set by every exec, 1 on a
failed compile) behind `elephc_pcre2_v1_last_error()`. Only a LITERAL pattern with a `u`
modifier widens the checker type (literal_pattern_is_utf8 in preg_match.rs), so every other
preg call keeps its unboxed Str/Int result; codegen boxes only when the result type is Mixed.
Eval registers the reader separately (__elephc_eval_register_regex_last_error) because a 7th
argument to the provider registration would spill to the stack on x86 SysV.

Shim edit checklist (all three, or the old archive links with no error):
1. src/native_deps/catalog.rs PCRE2 recipe_revision
2. src/native_deps/recipe.rs built_in_recipe ("pcre2", N) + the not-dispatched test
3. elephc.lock `recipe = N`
then `elephc native install --locked --target macos-aarch64` (a build before that fails with
"native artifact is missing", and a probe script that does not delete its old binary silently
reruns the stale one).
Still open: preg_split/preg_match_all failure results, and preg_last_error()/_msg builtins.

## Why

Symfony console Helper::width tells binary strings apart by preg_replace(/u) returning null; and a shim edit without the bump compiles fine against the old archive
