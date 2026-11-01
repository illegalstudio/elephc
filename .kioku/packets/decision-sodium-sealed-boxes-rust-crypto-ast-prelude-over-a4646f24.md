---
id: decision-sodium-sealed-boxes-rust-crypto-ast-prelude-over-a4646f24
type: decision
title: "Sodium sealed boxes: Rust crypto, AST prelude over internal builtins for AOT, native eval homes that throw SodiumException by name"
description: "Why sodium_crypto_box_* is three layers, why SodiumException is injected for any eval program, and why the prelude is injected twice"
created: 2026-09-26
sources:
  - path: src/sodium_prelude.rs
    blob: ed6730e6f47b27fe5144f262966a00dbc3456b0c
    lines: 90-240
    snip: 4c1867265352
    anchor: "fn function_declarations() -> Vec<Stmt> {"
  - path: crates/elephc-magician/src/interpreter/builtins/string/sodium_box.rs
    blob: fa94ffe661e61257169d93831b8a72810b7ab187
    lines: 36-95
    snip: f1a08a568fe6
    anchor: "pub(in crate::interpreter) fn eval_sodium_box_result("
  - path: crates/elephc-crypto/src/sodium.rs
    blob: 92a11e21a500ab04b0bc4f9f0c366474d844eac9
    lines: 47-85
    snip: 581aa7c065c2
    anchor: "pub fn sodium_op(op: u32, first: &[u8], second: &[u8]) -> Result<Vec<u8>, i32> {"
---

# Sodium sealed boxes: Rust crypto, AST prelude over internal builtins for AOT, native eval homes that throw SodiumException by name

## Fact

`sodium_crypto_box_keypair/publickey/seal/seal_open` and `SodiumException` are delivered in three
layers, and the split is deliberate:

- Cryptography: `elephc_crypto::sodium` (RustCrypto `crypto_box` 0.9 with `seal`), one C entry
  `elephc_crypto_sodium(op, a, alen, b, blen, out, cap, &outlen) -> status`. Byte-compatible with
  libsodium: a unit test opens a box php 8.5 sealed, and the compiled secrets vault is read by php.
- AOT: `sodium_prelude` (AST-built) declares the class and the four functions over two internal
  builtins, `__elephc_sodium_box` (returns an owned string, empty on failure) and
  `__elephc_sodium_status` (reads the `_elephc_sodium_status` slot `__rt_sodium_box` writes). The
  PHP wrappers turn a status into `SodiumException` or `false`, so no assembly ever throws.
- Eval: the interpreter never runs a compiler prelude, so Magician implements the four names
  itself (`interpreter/builtins/string/sodium_box.rs`, listed in
  `EVAL_IMPLEMENTED_PRELUDE_SURFACES`) and throws `SodiumException` BY NAME. That only works if
  the host program declares the class, which is why the prelude injects the class alone whenever
  the program contains `eval`/runtime include (`used.introspects || used.includes_runtime_php`).

Two injection sites: beside the hash prelude BEFORE name resolution (a call written
`\SODIUM_CRYPTO_BOX_SEAL()` only folds onto the declared function if it exists when names
resolve), and again in the late compat-prelude phase for references only autoloaded code makes.
The test harness (`tests/codegen/support/compiler.rs`) mirrors both; forgetting the mirror made
every test report "Call to undefined function" while the real binary worked.

## Why

Symfony's secrets:* commands run SodiumVault interpreted; the class must exist in the host program and the double injection is what makes a case-folded call resolve
