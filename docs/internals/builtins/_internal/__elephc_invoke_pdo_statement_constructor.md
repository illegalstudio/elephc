---
title: "__elephc_invoke_pdo_statement_constructor() — internals"
description: "Compiler internals for __elephc_invoke_pdo_statement_constructor(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 442
---

## `__elephc_invoke_pdo_statement_constructor()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/__elephc_invoke_pdo_statement_constructor.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/__elephc_invoke_pdo_statement_constructor.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:589](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L589) (`lower_elephc_invoke_pdo_statement_constructor`)
- **Function symbol**: `lower_elephc_invoke_pdo_statement_constructor()`


### Lowering notes

- Invokes a selected PDOStatement subclass constructor after its native state is initialized.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_strtotime`

## Signature summary

```php
function __elephc_invoke_pdo_statement_constructor(string $class, mixed $statement, mixed $arguments): void
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
