---
title: "define() — internals"
description: "Compiler internals for define(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 275
---

## `define()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:591](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L591) (`lower_define`)
- **Function symbol**: `lower_define()`


### Lowering notes

- Lowers `define("NAME", value)` with the legacy duplicate-name runtime guard.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function define(string $constant_name, mixed $value, bool $case_insensitive): bool
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `define()`](../../../php/builtins/misc/define.md)

