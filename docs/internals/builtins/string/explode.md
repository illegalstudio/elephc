---
title: "explode() — internals"
description: "Compiler internals for explode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 327
---

## `explode()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:151](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L151) (`lower_explode`)
- **Function symbol**: `lower_explode()`


### Lowering notes

- Lowers `explode(delimiter, string)` into the shared string-array splitter helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_explode`
- `__rt_sscanf`

## Signature summary

```php
function explode(string $separator, string $string, int $limit): array
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `explode()`](../../../php/builtins/string/explode.md)

