---
title: "token_name() - internals"
description: "Compiler internals for token_name(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 962
---

## `token_name()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/tokenizer_prelude.rs`:77](https://github.com/illegalstudio/elephc/blob/main/src/tokenizer_prelude.rs#L77) (`token_name`)
- **Function symbol**: `token_name()`


### Lowering notes

- Implemented by the compiler-injected tokenizer prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function token_name(int $id): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/tokenizer/token_name.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/tokenizer/token_name.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `token_name()`](../../../php/builtins/string/token_name.md)
