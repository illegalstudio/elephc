---
title: "__elephc_phar_get_signature_hash() — internals"
description: "Compiler internals for __elephc_phar_get_signature_hash(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 439
---

## `__elephc_phar_get_signature_hash()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/__elephc_phar_get_signature_hash.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/__elephc_phar_get_signature_hash.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4192](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4192) (`lower_elephc_phar_get_signature_hash`)
- **Function symbol**: `lower_elephc_phar_get_signature_hash()`


### Lowering notes

- Lowers `__elephc_phar_get_signature_hash(path)` into the signature-hash read bridge.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_get_signature_hash(string $path): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
