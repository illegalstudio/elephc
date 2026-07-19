---
title: "__elephc_initialize_pdo_statement() — internals"
description: "Compiler internals for __elephc_initialize_pdo_statement(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 441
---

## `__elephc_initialize_pdo_statement()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/__elephc_initialize_pdo_statement.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/__elephc_initialize_pdo_statement.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:598](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L598) (`lower_elephc_initialize_pdo_statement`)
- **Function symbol**: `lower_elephc_initialize_pdo_statement()`


### Lowering notes

- Initializes the private PDOStatement base fields on a dynamically allocated subclass.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_strtotime`

## Signature summary

```php
function __elephc_initialize_pdo_statement(mixed $statement, int $handle, int $connection, int $errorMode, string $query): void
```

## What the type checker enforces

- **Arity**: takes exactly 5 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
