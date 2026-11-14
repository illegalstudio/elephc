---
title: "unpack() - internals"
description: "Compiler internals for unpack(): lowering path, type checks, and runtime helpers."
---

## `unpack()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/pack_prelude.rs`](https://github.com/illegalstudio/elephc/blob/main/src/pack_prelude.rs) (`unpack`)
- **Function symbol**: `unpack()`


### Lowering notes

- Implemented by the compiler-injected pack prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function unpack(string $format, string $string, int $offset = 0): array|false
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `unpack()`](../../../php/builtins/string/unpack.md)
