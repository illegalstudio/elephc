---
title: "__elephc_phar_sign_hash() — internals"
description: "Compiler internals for __elephc_phar_sign_hash(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 445
---

## `__elephc_phar_sign_hash()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4163](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4163) (`lower_elephc_phar_sign_hash`)
- **Function symbol**: `lower_elephc_phar_sign_hash()`


### Lowering notes

- Lowers `__elephc_phar_sign_hash(path, algo)` into the hash-based signing bridge.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_sign_hash(mixed $path, mixed $algo): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
