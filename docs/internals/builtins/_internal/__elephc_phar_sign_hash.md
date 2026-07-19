---
title: "__elephc_phar_sign_hash() — internals"
description: "Compiler internals for __elephc_phar_sign_hash(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 467
---

## `__elephc_phar_sign_hash()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/__elephc_phar_sign_hash.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/__elephc_phar_sign_hash.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4160](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4160) (`lower_elephc_phar_sign_hash`)
- **Function symbol**: `lower_elephc_phar_sign_hash()`


### Lowering notes

- Lowers `__elephc_phar_sign_hash(path, algo)` into the hash-based signing bridge.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_sign_hash(string $path, string $algo): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
