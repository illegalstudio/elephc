---
title: "__elephc_phar_get_metadata() — internals"
description: "Compiler internals for __elephc_phar_get_metadata(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 419
---

## `__elephc_phar_get_metadata()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3853](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3853) (`lower_elephc_phar_get_metadata`)
- **Function symbol**: `lower_elephc_phar_get_metadata()`


### Lowering notes

- Lowers `__elephc_phar_get_metadata()` into the metadata-read bridge call.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_get_metadata(mixed $filename): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
