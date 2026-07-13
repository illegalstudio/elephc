---
title: "ucwords() — internals"
description: "Compiler internals for ucwords(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 408
---

## `ucwords()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/ucwords.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/ucwords.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:76](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L76) (`lower_unary_string_runtime`)
- **Function symbol**: `lower_unary_string_runtime()`


### Lowering notes

- Lowers a one-argument string builtin that directly delegates to a runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_htmlspecialchars`

## Signature summary

```php
function ucwords(string $string, string $separators = ' \t\r\n\x0c\x0b'): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/ucwords.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ucwords.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ucwords()`](../../../php/builtins/string/ucwords.md)
