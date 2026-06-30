---
title: "__elephc_phar_get_stub() — internals"
description: "Compiler internals for __elephc_phar_get_stub(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 437
---

## `__elephc_phar_get_stub()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3873](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3873) (`lower_elephc_phar_get_stub`)
- **Function symbol**: `lower_elephc_phar_get_stub()`


### Lowering notes

- Lowers `__elephc_phar_get_stub()` into the stub-read bridge call.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_get_stub(mixed $filename): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
