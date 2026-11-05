---
title: "Sodium: full ext/sodium over libsodium 1.0.22"
author: Guillaume
date: 2026-11-04
branch: sodium-php-src-compliance
cost: { minutes: 240, tokens: 160000 }
touches:
  - Cargo.lock
  - Cargo.toml
  - crates/elephc-builtin-contract/src/catalog_classes.rs
  - crates/elephc-builtin-contract/src/catalog_constants.rs
  - crates/elephc-builtin-contract/src/catalog_password.rs
  - crates/elephc-builtin-contract/src/catalog_sodium.rs
  - crates/elephc-builtin-contract/src/lib.rs
  - crates/elephc-builtin-contract/src/registry.rs
  - crates/elephc-builtin-contract/src/requirements.rs
  - crates/elephc-builtin-contract/src/spec.rs
  - crates/elephc-builtin-contract/src/support.rs
  - crates/elephc-magician/src/interpreter/builtins/sodium/dispatch.rs
  - crates/elephc-magician/src/interpreter/builtins/sodium/mod.rs
  - crates/elephc-sodium/src/ffi.rs
  - crates/elephc-sodium/src/lib.rs
  - crates/elephc-sodium/src/ops.rs
  - src/ir_lower/expr/call_arg_coercion.rs
  - src/sodium_prelude/password.rs
  - src/sodium_prelude/eval_wrappers.rs
  - scripts/gen_php_src_sodium_cases.py
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
- The eight by-reference mutators now keep php-src's real `string &$...` contract, with
  `by_ref: true` and `TypeSpec::Str`. The shared argument lowering must skip its usual
  scalar cast for every by-reference parameter: casting materializes a temporary, so the
  callee would mutate a copy. `src/ir_lower/expr/call_arg_coercion.rs` returns the
  original lvalue for `sig.ref_params`; `Standard` lowering plus `store_operand_local`
  still writes the advanced string/null back. Do not "fix" this by widening the public
  parameter to `mixed` and hiding it in the docs table.
- `sodium_crypto_pwhash` has a dedicated six-register extern (`password`, `salt`,
  `length`, `ops`, `mem`, `algo`): the generic bridge entry has room for only three
  integers after `op` + four strings.
- Core `password` is not a second implementation: the sodium prelude declares
  `password_hash()`, `password_verify()`, `password_get_info()`,
  `password_needs_rehash()` and `password_algos()`, with `PASSWORD_*` constants and five
  `PreludeProvided` contracts. `password_hash()` uses the dedicated
  `elephc_sodium_pwhash_str_alg` extern (1 = Argon2i, 2 = Argon2id), so the Argon2 string
  format and option checks match php-src's sodium password adapter.
- The php-src PHPT corpus is migrated as 31 inline Rust tests
  (`tests/php_src/sodium/` provenance, `scripts/gen_php_src_sodium_cases.py`,
  `tests/codegen/sodium_php_src*.rs`). All 31 pass with no ignores, including the three
  `password_hash` interoperability fixtures and `bug78516`.
- Eval forwarding covers all 110 functions. The eight by-reference functions run through
  hidden `__elephc_sodium_eval_*` prelude wrappers with ordinary PHP `&$` parameters; eval's
  normal user-function call machinery mutates the caller variable. The public eval bindings
  set `source_arguments: true` so the direct hook selects this path. Do not try to write
  back from an `EvalValuesHook`: it receives values, not caller lvalues.

## Tried and ruled out
- `BuiltinArgumentLowering::PreserveUncoerced` for the mutators: it released the source
  operand after preparing a copy, so `store_operand_local` either failed or stored a
  dangling value. `Standard` keeps the lvalue and fixed both symptoms.
- Synthetic public PHP wrappers with `&$state`: normal call lowering passed a plain
  string where the callee expected a reference cell, producing garbage. The registry
  route with the real `string &` contract plus the lvalue-preserving coercion is the
  working shape.
- Returning `mixed` from the by-ref contracts and adding a docs override: it made the
  reflection/docs surface lie and only papered over the coercion bug. The real signature
  works once `lower_arg_with_signature` stops casting reference arguments.
- Encoding the pwhash algorithm in a spare string slot: the generated `(string)int`
  argument arrived empty at the bridge. A dedicated extern is reliable.
- A `char*`-based bridge: NUL bytes inside keys/ciphertexts truncate. Hex transport is
  deliberate, not a temporary shortcut.
- Implementing the Argon2 provider twice: the sodium bridge already owns
  `crypto_pwhash_str_alg`; the core `password_*` functions are synthetic prelude wrappers
  over that one implementation, and their contracts mark the sodium bridge requirement.

## Watch out
- Dispatch order in `crates/elephc-sodium/src/ops.rs`: the `_keygen`/`_keypair` guard
  must come BEFORE the generic `sodium_crypto_aead_*` guard, otherwise every AEAD
  keygen is routed into `aead()` and reports `internal error`.
- `get_extension_funcs('sodium')` needed a branch in
  `src/codegen/lower_inst/core_builtins/introspection.rs`; it previously only knew
  `Core`. The dynamic-name path needs its own label so it does not return the Core list.
- Native eval registration deliberately skips names starting with `_`; sodium helpers
  need the same explicit allowlist as `__elephc_gmp_*` in
  `src/codegen/lower_inst/builtins/eval/signature_metadata.rs`. The hidden eval wrappers
  are in that allowlist too, or eval reports `Call to undefined function`.
- libsodium 1.0.22 rejects Argon2i with `time_cost` 1 or 2 at small memory sizes even
  though PHP's bundled libargon2 accepts them. The sodium PHPTs use 4 and 8; the focused
  provider test uses 4. A rejected libsodium call becomes
  `ValueError: Unexpected failure hashing password`, matching the sodium adapter.
- `password_needs_rehash()` with invalid options returns true (not a ValueError), and an
  unknown algorithm returns false; both match the core `password.c` provider-selection
  logic. PHP core can hash bcrypt and the sodium adapter cannot; `PASSWORD_BCRYPT` is
  advertised by `password_algos()`/`password_get_info()` but hashing `2y` is outside this
  provider.
- libsodium 1.0.22 exposes AEGIS-128L/256 and reports `SODIUM_LIBRARY_VERSION=1.0.22`,
  major 26, minor 4. The committed php baseline was 1.0.18 (major 10/minor 3), so the
  AEGIS functions/constants and the version triple were merged into
  `scripts/docs/php_baseline.json` rather than dropping unloaded extensions by
  re-snapshotting the whole PHP build.
- The docs extractor needs a `PRELUDE_SOURCES` and `REGISTRY_AREA_DEFAULTS` entry for
  every prelude area. `password_*` uses `Area::Password`, a `Password` docs area and
  `sodium_prelude/password.rs`; without them the generated registry silently lacked the
  five pages even though `gen_builtins` emitted the contracts.
- `render.py` computed a `prefix` for by-reference parameters but never used it, so every
  generated signature omitted `&`. The sodium docs override had hidden this; fix the
  renderer (and regenerate) rather than reintroducing per-builtin type overrides.
- A strict `TypeError` raised inside a nested prelude helper (helper -> string_param)
  is not surfaced by the current runtime when the caller is a user function. The
  `sodium_error_001` migration calls the builtin directly; this is a runtime limitation,
  not a sodium argument bug.
- PHPT tests are heavy (Argon2 + many links). `tests/codegen/sodium_php_src.rs` raises the
  per-binary timeout to 300 seconds and the corpus is run with `--test-threads=2`.
  Higher parallelism produced transient `assembler failed` panics and 60-second timeouts.
