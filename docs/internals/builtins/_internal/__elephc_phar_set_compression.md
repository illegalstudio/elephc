---
title: "__elephc_phar_set_compression() — internals"
description: "Compiler internals for __elephc_phar_set_compression(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 439
---

## `__elephc_phar_set_compression()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/__elephc_phar_set_compression.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/__elephc_phar_set_compression.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3801](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3801) (`lower_elephc_phar_set_compression`)
- **Function symbol**: `lower_elephc_phar_set_compression()`


### Lowering notes

- Internal helper used by the built-in Phar / PharData support to change archive compression.
- Calls the native PHAR compression-control bridge and returns whether the update succeeded.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_set_compression(string $filename, int $compression): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
