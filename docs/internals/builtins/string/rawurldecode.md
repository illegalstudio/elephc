---
title: "rawurldecode() — internals"
description: "Compiler internals for rawurldecode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 394
---

## `rawurldecode()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/rawurldecode.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/rawurldecode.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:75](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L75) (`lower_unary_string_runtime`)
- **Function symbol**: `lower_unary_string_runtime()`


### Lowering notes

- Lowers a one-argument string builtin that directly delegates to a runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_htmlspecialchars`

## Signature summary

```php
function rawurldecode(string $string): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/rawurldecode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/rawurldecode.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `rawurldecode()`](../../../php/builtins/string/rawurldecode.md)
