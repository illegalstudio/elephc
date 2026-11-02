---
title: Worker deferral must forget memos naming chunk data; php_site is ABI-mangled
author: Guillaume
date: 2026-11-01
branch: feat/closed-world-symfony-build-speed
touches:
  - src/codegen/block_emit.rs
  - src/ir_lower/tests/php_site_symbol.rs
---

## Decided
- A codegen worker that defers a body rolls that body's text AND data back (`emit_chunk` in
  `src/codegen/block_emit.rs`), but `SharedCodegenState` caches are not rolled back. Every memo
  whose value NAMES a label in the chunk's `DataSection` must therefore be cleared in
  `SharedCodegenState::forget_worker_templates`, which runs on every deferral. That now covers the
  descriptor templates, the keyed helpers, `instance_lookup_tables` and `inaccessible_method_table`.
- Memos of pure module facts (`mixed_method_candidates`, the class-key indexes, `type_is_a_memo`)
  name no data and stay.
- `php_site` is the one runtime data symbol Rust reads (Magician's `compiled_site.rs`). Its
  definition and every assembly reference go through `abi::php_site_symbol(&target)`
  (`Target::extern_symbol("php_site")`): `_php_site` on Mach-O, `php_site` on ELF.

## Tried and ruled out
- Suspecting `DataSection::merge` / `rollback_to`: both are sound. The orphan was a memo that
  outlived the rollback, not a lost merge entry.

## Watch out
- Symptom: link fails on `_data_h<16 hex>` symbols referenced from bodies such as
  `Configuration::addAssetsSection` or `PhpFileLoader::callConfigurator`. `_data_h…` labels exist
  only in parallel shards (content-addressed); the serial pass spells tables `_data_N`, so it can
  never define one. A Symfony `--web` build WITHOUT preload reproduces it on macOS in ~30 s.
- Any NEW memo in `SharedCodegenState` holding a data label needs a line in
  `forget_worker_templates` (regression: `deferral_forgets_memos_naming_chunk_data`).
- A runtime data symbol read from Rust must not be spelled with a literal `_` prefix in asm: it
  links on Apple only (regression: `src/ir_lower/tests/php_site_symbol.rs`).
