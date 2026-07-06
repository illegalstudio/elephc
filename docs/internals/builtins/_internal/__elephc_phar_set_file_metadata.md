---
title: "__elephc_phar_set_file_metadata() — internals"
description: "Compiler internals for __elephc_phar_set_file_metadata(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 440
---

## `__elephc_phar_set_file_metadata()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/__elephc_phar_set_file_metadata.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/__elephc_phar_set_file_metadata.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4090](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4090) (`lower_elephc_phar_set_file_metadata`)
- **Function symbol**: `lower_elephc_phar_set_file_metadata()`


### Lowering notes

- Lowers `__elephc_phar_set_file_metadata()` into the per-file metadata-write bridge.
- The single `phar://archive/entry` URL argument is split by the bridge, so this
- reuses the same `(url, data) -> bool` shape as the archive-level metadata writer.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_set_file_metadata(string $url, string $metadata): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
