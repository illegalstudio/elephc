---
title: "Sodium: full ext/sodium over libsodium 1.0.22"
author: Guillaume
date: 2026-11-04
branch: sodium-php-src-compliance
cost: { minutes: 180, tokens: 120000 }
touches:
  - Cargo.lock
  - Cargo.toml
  - crates/elephc-builtin-contract/src/catalog_classes.rs
  - crates/elephc-builtin-contract/src/catalog_constants.rs
  - crates/elephc-builtin-contract/src/lib.rs
  - crates/elephc-builtin-contract/src/registry.rs
  - crates/elephc-builtin-contract/src/requirements.rs
  - crates/elephc-builtin-contract/src/spec.rs
  - crates/elephc-builtin-contract/src/support.rs
  - crates/elephc-magician/src/interpreter/builtins/hooks/direct.rs
---

## Decided
- ext/sodium is a managed native `libsodium` 1.0.22 package plus a Rust `elephc_sodium`
  staticlib whose `extern "C"` symbols stay unresolved until the final program link.
  No system `-lsodium` fallback; a missing package fails closed with
  `elephc native add libsodium`.
- The complete PHP 8.5 surface is synthetic AST: 110 procedural helpers, 102 `SODIUM_*`
  constants (also registered in `catalog_constants`), and a synthetic
  `SodiumException extends Exception`.
- Binary data crosses the C ABI as hexadecimal text. Every `CType::Str` parameter is a
  `char*`, so passing raw binary would truncate at the first NUL. Helpers encode with
  `bin2hex` and decode the bridge packet with `hex2bin`.
- The eight by-reference mutators stay registry builtins, but their first parameter is
  `TypeSpec::Mixed` with `by_ref: true` and the AOT semantics use `Standard` argument
  lowering. `mixed` keeps the caller's lvalue from being cast into a temporary; the
  helper owns php-src's real `string` coercion and the lowering writes state back with
  `store_operand_local`. This is what made incremental BLAKE2b and secretstream work.
- `sodium_crypto_pwhash` has a dedicated six-register extern (`password`, `salt`,
  `length`, `ops`, `mem`, `algo`): the generic bridge entry has room for only three
  integers after `op` + four strings.
- The php-src PHPT corpus is migrated as 31 inline Rust tests
  (`tests/php_src/sodium/` provenance, `scripts/gen_php_src_sodium_cases.py`,
  `tests/codegen/sodium_php_src*.rs`). 27 pass; 3 `password_hash` fixtures and
  `bug78516` are ignored because elephc has no core `password` extension.
- Eval forwarding covers the 102 non-mutating functions. The eight by-reference
  functions are explicitly `EVAL_IMPLEMENTATION_PENDING` because the eval builtin
  boundary receives values, not caller lvalues.

## Tried and ruled out
- `BuiltinArgumentLowering::PreserveUncoerced` for the mutators: it released the source
  operand after preparing a copy, so `store_operand_local` either failed or stored a
  dangling value. `Standard` keeps the lvalue and fixed both symptoms.
- Synthetic public PHP wrappers with `&$state`: normal call lowering passed a plain
  string where the callee expected a reference cell, producing garbage. The registry
  route with `mixed &` is the working shape.
- Encoding the pwhash algorithm in a spare string slot: the generated `(string)int`
  argument arrived empty at the bridge. A dedicated extern is reliable.
- A `char*`-based bridge: NUL bytes inside keys/ciphertexts truncate. Hex transport is
  deliberate, not a temporary shortcut.

## Watch out
- Dispatch order in `crates/elephc-sodium/src/ops.rs`: the `_keygen`/`_keypair` guard
  must come BEFORE the generic `sodium_crypto_aead_*` guard, otherwise every AEAD
  keygen is routed into `aead()` and reports `internal error`.
- `get_extension_funcs('sodium')` needed a branch in
  `src/codegen/lower_inst/core_builtins/introspection.rs`; it previously only knew
  `Core`. The dynamic-name path needs its own label so it does not return the Core list.
- Native eval registration deliberately skips names starting with `_`; sodium helpers
  need the same explicit allowlist as `__elephc_gmp_*` in
  `src/codegen/lower_inst/builtins/eval/signature_metadata.rs`.
- libsodium 1.0.22 exposes AEGIS-128L/256 and reports `SODIUM_LIBRARY_VERSION=1.0.22`,
  major 26, minor 4. The committed php baseline was 1.0.18 (major 10/minor 3), so the
  AEGIS functions/constants and the version triple were merged into
  `scripts/docs/php_baseline.json` rather than dropping unloaded extensions by
  re-snapshotting the whole PHP build.
- A strict `TypeError` raised inside a nested prelude helper (helper -> string_param)
  is not surfaced by the current runtime when the caller is a user function. The
  `sodium_error_001` migration calls the builtin directly; this is a runtime limitation,
  not a sodium argument bug.
- PHPT tests are heavy (Argon2 + many links); locally run with `--test-threads=4`.
  Higher parallelism produced transient `assembler failed` panics.
