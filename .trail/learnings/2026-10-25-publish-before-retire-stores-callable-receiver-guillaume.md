---
title: "Publish-before-retire stores, callable receiver liveness, magic silent probes, exact php patch version"
author: Guillaume
date: 2026-10-25
branch: reconcile/dirname-symfony
touches:
  - src/locale_prelude.rs
  - elephc.toml
---

## Decided
- Overwrites are publish-before-retire everywhere php's assignment is: symbol stores of refcounted values (`abi::emit_store_refcounted_result_to_symbol`), `__rt_hash_set` updates, `__rt_mixed_array_set` overwrites. A destructor run by the release may write the same place; `__rt_mixed_array_set` keeps the table the CELL holds when a reentrant write grew it (the helper's returned table is stale then).
- Liveness: a callable's captured receiver is used where the callback is consumed (`usort`, `array_map`), not where it was built. `ir_passes::intervals::callable_captures` tracks FirstClassCallableNew / two-slot `[$o,'m']` literals through Acquire and locals function-wide (union over stores, fixed point); a use placed before the def extends to the end (loops).
- Silent property probes: `Probe` = `isset()` = `__isset` alone (answered as boxed true/null); `empty()` and `??` use `Coalesce` (`__isset` then `__get`). A typed receiver only gets the direct slot read when the scope may read the slot.
- `--php-version 8.5.10` = the 8.5 profile reporting patch 10 to the program (`PhpVersion::reported_*`); `version_id()` stays minor-granular for resolution, gates and caches.
- Built-in `Attribute::TARGET_*` / `IS_REPEATABLE` fold to ints (and `|`/`&`/`^` of them) in `types::schema::fold_attr_value`. `checker::schema::classes::state` had its own copy of the folding; it now uses the shared one.
- intl `Locale` getDefault/setDefault is an AST prelude (`src/locale_prelude.rs`) following ICU's POSIX resolution; the rest of intl stays absent.

## Tried and ruled out
- A per-block capture map for the interval fix: the consumer is often in another block (foreach in between); only the function-wide union is safe.
- Changing `PhpVersion::version_id()` for the patch: profile resolution compares ids, a pinned patch broke it.
- Making `empty` a "lazy construct" in `builtins/mod.rs`: not where the refusal came from; it was the eager effects pass (`inference/expr/effects.rs`).

## Watch out
- `x86_64` `__rt_object_new_template` read its template from rax instead of rdi: every `new` was garbage on linux-x86_64 in the uncommitted tree. Probe x86 with real `docker` runs (scratchpad `x86one.sh`), not just assembly.
- `e_static_call("self", ...)` in a prelude is a call on a class NAMED self; use `e_self_call`. Only an app with an autoloader shows it.
- A stuck compile at 0 % CPU with a dyld-only stack is syspolicyd saturation, often from an orphaned `codegen_tests` whose shell timed out.
- `build-sfweb-asis.sh` interrupted mid-build left `elephc.toml` in the app; the next build waited on it forever (now trapped).
- Open, pre-existing (c300/c343): `ReflectionMethod::invoke` / `Closure::bind` scope for private ancestor methods (compile-time scope table); string written through a typed `User &$x` widened to `mixed &`; destructors of by-ref default cells run after the catch; `null|SplFixedArray` element write bad refcount; spread into a by-ref parameter.
