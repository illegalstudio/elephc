---
title: "define() — internals"
description: "Compiler internals for define(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 276
---

## `define()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/define.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/define.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:370](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L370) (`lower_define`)
- **Function symbol**: `lower_define()`


### Lowering notes

- Lowers `define("NAME", value)` with the duplicate-name runtime guard.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function define(string $constant_name, mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/core/define.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/define.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `define()`](../../../php/builtins/misc/define.md)
