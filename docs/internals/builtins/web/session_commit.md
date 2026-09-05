---
title: "session_commit() — internals"
description: "Compiler internals for session_commit(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 853
---

## `session_commit()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:3687](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L3687) (`session_commit`)
- **Function symbol**: `session_commit()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function session_commit(): bool
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `session_commit()`](../../../php/builtins/web/session_commit.md)
