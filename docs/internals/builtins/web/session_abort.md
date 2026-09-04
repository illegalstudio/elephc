---
title: "session_abort() — internals"
description: "Compiler internals for session_abort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 850
---

## `session_abort()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3373](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3373) (`session_abort`)
- **Function symbol**: `session_abort()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_abort(): bool
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_abort()`](../../../php/builtins/web/session_abort.md)
