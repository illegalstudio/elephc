---
title: "ini_get() — internals"
description: "Compiler internals for ini_get(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 847
---

## `ini_get()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:4356](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L4356) (`ini_get`)
- **Function symbol**: `ini_get()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function ini_get(string $option): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `ini_get()`](../../../php/builtins/web/ini_get.md)
