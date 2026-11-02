---
title: A failed native registration poisons the eval catalog; unscoped closure variants
author: Guillaume
date: 2026-11-02
branch: feat/closed-world-symfony-build-speed
touches:
  - crates/elephc-magician/src/context/native_catalog.rs
---

## Decided
- One failed native registration poisons the eval catalog: `publish_native_catalog`
  (`crates/elephc-magician/src/context/native_catalog.rs`) refuses when
  `native_catalog_registration_failed` is set, so no context ever attaches the cached catalog and
  every new eval context replays the WHOLE inventory (`_elephc_eval_register_inventory`). Under
  Symfony's `opcache.preload` that made a `--web` binary spin at 100% CPU before listening.
- The failure was a DNF parameter type: the compiler spells `(A&B)|null` as `A&B|null`, which
  `native_callable_type_from_spec` refused. It now decodes as `intersection_allowing_null`; other
  DNF shapes (`(A&B)|C`) still cannot be represented and stay refused.
- A NO-scope closure variant (planned for every `$this`-capturing closure when an untraced
  `Closure::bind($fn, null, null)` site exists) must lower `self::$x` to php's run-time
  `Cannot access "self" when no class scope is active`, never refuse the build.
  `eval_dynamic_static_property_target` no longer propagates the receiver resolution error, and
  `static_prop_initialized` emits the scope error.

## Tried and ruled out
- Slices each owning their own `L__elephc_eval_native_catalog` token: `asm_split` promotes a
  cross-slice `L` label to one symbol, all contexts share one generation.
- A failure in `fail_native_catalog_registration` caught by an lldb regex breakpoint: it is
  inlined, the breakpoint never fires. An env-gated eprintln in `publish_native_catalog` showed
  `failed=true` at once.

## Watch out
- Symptom of a poisoned catalog: startup or first request at 100% CPU, samples almost all under
  `_elephc_eval_register_inventory` called from ordinary compiled functions.
- Symfony builds must use `elephc-scratch/tools/build-sfweb-asis.sh` (Symfony's own
  `App_KernelProdContainer.preload.php`, fresh `cache:clear`, `opcache.enable_cli=1`), not the
  README's wrapper preload; a stale `var/cache/dev` from a `php -S` without `-n` (dev mode) gets
  compiled too.
- Open: eval reflection renders `(A&B)|null` as `A&B` (no DNF in `ReflectionUnionType`), and a
  method inherited from a parent interface reports the child interface as its declaring class.
