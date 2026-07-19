---
title: "__elephc_pdo_called_class_status() — internals"
description: "Compiler internals for __elephc_pdo_called_class_status(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 447
---

## `__elephc_pdo_called_class_status()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/__elephc_pdo_called_class_status.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/__elephc_pdo_called_class_status.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:580](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L580) (`lower_elephc_pdo_called_class_status`)
- **Function symbol**: `lower_elephc_pdo_called_class_status()`


### Lowering notes

- Classifies the late-static called class for `PDO::connect()` driver validation.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_strtotime`

## Signature summary

```php
function __elephc_pdo_called_class_status(string $class): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
