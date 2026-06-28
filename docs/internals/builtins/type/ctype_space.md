---
title: "ctype_space() — internals"
description: "Compiler internals for ctype_space(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 395
---

## `ctype_space()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/ctype.rs`:35](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/ctype.rs#L35) (`lower_ctype_space`)
- **Function symbol**: `lower_ctype_space()`


### Lowering notes

- Lowers `ctype_space(string)` by checking every byte against PHP's ASCII whitespace set.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ctype_space(string $text): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `ctype_space()`](../../../php/builtins/type/ctype_space.md)

