---
title: "fsockopen() — internals"
description: "Compiler internals for fsockopen(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 350
---

## `fsockopen()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fsockopen.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fsockopen.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3641](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3641) (`lower_fsockopen`)
- **Function symbol**: `lower_fsockopen()`


### Lowering notes

- Lowers `fsockopen(host, port, errno?, errstr?, timeout?)`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function fsockopen(string $hostname, int $port, int $error_code = null, string $error_message = null, float $timeout = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–5 arguments (3 optional).
- **By-reference parameters**: `$error_code`, `$error_message`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fsockopen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fsockopen.rs) (`eval_builtin!`)
- **Dispatch hooks**: `values`
- **By-reference parameters**: `$error_code`, `$error_message`.

## Cross-references

- [User reference for `fsockopen()`](../../../php/builtins/streams/fsockopen.md)
