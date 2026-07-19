---
title: "strrpos() — internals"
description: "Compiler internals for strrpos(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 414
---

## `strrpos()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/strrpos.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/strrpos.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:763](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L763) (`lower_string_position`)
- **Function symbol**: `lower_string_position()`


### Lowering notes

- Lowers `strpos()`/`strrpos()` and boxes position-or-false results as Mixed.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function strrpos(string $haystack, string $needle, int $offset = 0): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/strrpos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strrpos.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `strrpos()`](../../../php/builtins/string/strrpos.md)
