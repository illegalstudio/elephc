---
title: "long2ip() — internals"
description: "Compiler internals for long2ip(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 350
---

## `long2ip()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:476](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L476) (`lower_long2ip`)
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

## Cross-references

- [User reference for `long2ip()`](../../../php/builtins/string/long2ip.md)

