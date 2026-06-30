---
title: "strstr() — internals"
description: "Compiler internals for strstr(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 395
---

## `strstr()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:762](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L762) (`lower_strstr`)
- **Function symbol**: `lower_strstr()`


### Lowering notes

- Lowers `strstr(haystack, needle)` by searching and returning the matching suffix.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function strstr(string $haystack, string $needle, bool $before_needle): string
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `strstr()`](../../../php/builtins/string/strstr.md)

