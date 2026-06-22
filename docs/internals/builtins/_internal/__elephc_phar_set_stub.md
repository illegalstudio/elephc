---
title: "__elephc_phar_set_stub() — internals"
description: "Compiler internals for __elephc_phar_set_stub(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 428
---

## `__elephc_phar_set_stub()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3890](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3890) (`lower_elephc_phar_set_stub`)
- **Function symbol**: `lower_elephc_phar_set_stub()`


### Lowering notes

- Lowers `__elephc_phar_set_stub()` into the stub-write bridge call.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_set_stub(mixed $filename, mixed $stub): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
