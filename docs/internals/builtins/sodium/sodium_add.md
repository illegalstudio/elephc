---
title: "sodium_add() - internals"
description: "Compiler internals for sodium_add(): lowering path, type checks, and runtime helpers."
---

## `sodium_add()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_sodium.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_sodium.rs)
- **Lowering**: [`src/sodium_prelude/mutators.rs`](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude/mutators.rs) (`sodium_add`)
- **Function symbol**: `sodium_add()`


### Lowering notes

- Implemented by the compiler-injected sodium prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function sodium_add(string &$string1, string $string2): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$string1`.

## Eval interpreter (magician)

Eval resolves the same native prelude declaration the AOT program calls ([`src/sodium_prelude/mutators.rs`](https://github.com/illegalstudio/elephc/blob/main/src/sodium_prelude/mutators.rs)); there is no separate `eval_builtin!` binding, so named and dynamic calls share the compiled by-reference call machinery.

## Cross-references

- [User reference for `sodium_add()`](../../../php/builtins/sodium/sodium_add.md)
