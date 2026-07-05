---
title: "readline() — internals"
description: "Compiler internals for readline(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 306
---

## `readline()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/readline.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/readline.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:310](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L310) (`lower_readline`)
- **Function symbol**: `lower_readline()`


### Lowering notes

- Lowers `readline(prompt?)` by optionally writing a prompt and reading stdin.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fgets`

## Signature summary

```php
function readline(string $prompt = null): mixed
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Cross-references

- [User reference for `readline()`](../../../php/builtins/process/readline.md)
