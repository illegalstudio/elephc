---
title: "long2ip() — internals"
description: "Compiler internals for long2ip(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 373
---

## `long2ip()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/long2ip.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/long2ip.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:539](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L539) (`lower_long2ip`)
- **Function symbol**: `lower_long2ip()`


### Lowering notes

- Lowers `long2ip(value)` through the IPv4 formatting runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ip2long`
- `__rt_long2ip`

## Signature summary

```php
function long2ip(int $ip): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/long2ip.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/long2ip.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `long2ip()`](../../../php/builtins/string/long2ip.md)
