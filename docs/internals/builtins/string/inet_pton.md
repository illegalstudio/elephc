---
title: "inet_pton() — internals"
description: "Compiler internals for inet_pton(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 383
---

## `inet_pton()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/inet_pton.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/inet_pton.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:559](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L559) (`lower_inet`)
- **Function symbol**: `lower_inet()`


### Lowering notes

- Lowers `inet_ntop()` and `inet_pton()` and boxes invalid-address results as PHP false.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_sprintf`

## Signature summary

```php
function inet_pton(string $ip): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/inet_pton.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/inet_pton.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `inet_pton()`](../../../php/builtins/string/inet_pton.md)
