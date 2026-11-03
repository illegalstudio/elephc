---
title: "proc_open() - internals"
description: "Compiler internals for proc_open(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 787
---

## `proc_open()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/proc_prelude.rs`:421](https://github.com/illegalstudio/elephc/blob/main/src/proc_prelude.rs#L421) (`proc_open`)
- **Function symbol**: `proc_open()`


### Lowering notes

- Implemented by the compiler-injected proc prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function proc_open(array|string $command, array $descriptor_spec, mixed $pipes, ?string $cwd = null, ?array $env_vars = null, ?array $options = null): mixed
```

## What the type checker enforces

- **Arity**: takes 3–6 arguments (3 optional).
- **By-reference parameters**: `$pipes`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_open.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_open.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `values`
- **By-reference parameters**: `$pipes`.

## Cross-references

- [User reference for `proc_open()`](../../../php/builtins/misc/proc_open.md)
