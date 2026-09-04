---
title: "session_gc() — internals"
description: "Compiler internals for session_gc(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 858
---

## `session_gc()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3343](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3343) (`session_gc`)
- **Function symbol**: `session_gc()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_gc(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_gc()`](../../../php/builtins/web/session_gc.md)
