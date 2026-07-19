---
title: "rawurlencode() — internals"
description: "Compiler internals for rawurlencode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 382
---

## `rawurlencode()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/rawurlencode.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/rawurlencode.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:76](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L76) (`lower_unary_string_runtime`)
- **Function symbol**: `lower_unary_string_runtime()`


### Lowering notes

- Lowers a one-argument string builtin that directly delegates to a runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_htmlspecialchars`

## Signature summary

```php
function rawurlencode(string $string): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/rawurlencode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/rawurlencode.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `rawurlencode()`](../../../php/builtins/string/rawurlencode.md)
