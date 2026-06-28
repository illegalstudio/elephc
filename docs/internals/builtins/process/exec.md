---
title: "exec() — internals"
description: "Compiler internals for exec(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 287
---

## `exec()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:690](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L690) (`lower_exec`)
- **Function symbol**: `lower_exec()`


### Lowering notes

- Lowers `exec(command)` by capturing shell stdout through the shared runtime helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function exec(string $command, array $output, int $result_code): string
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.
- **By-reference parameters**: `$output`, `$result_code`.

## Cross-references

- [User reference for `exec()`](../../../php/builtins/process/exec.md)

