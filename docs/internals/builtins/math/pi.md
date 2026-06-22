---
title: "pi() — internals"
description: "Compiler internals for pi(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 243
---

## `pi()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:623](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L623) (`lower_pi`)
- **Function symbol**: `lower_pi()`


### Lowering notes

- Lowers `pi()` as the same data-section float constant used by the legacy backend.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function pi(): float
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `pi()`](../../../php/builtins/math/pi.md)

