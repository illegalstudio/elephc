---
title: "ip2long() — internals"
description: "Compiler internals for ip2long(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 348
---

## `ip2long()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:488](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L488) (`lower_ip2long`)
- **Function symbol**: `lower_ip2long()`


### Lowering notes

- Lowers `ip2long(string)` and boxes invalid-address results as PHP false.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_ip2long`
- `__rt_sprintf`

## Signature summary

```php
function ip2long(string $ip): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `ip2long()`](../../../php/builtins/string/ip2long.md)

