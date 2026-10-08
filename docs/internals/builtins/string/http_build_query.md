---
title: "http_build_query() - internals"
description: "Compiler internals for http_build_query(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 811
---

## `http_build_query()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/http_build_query_prelude.rs`:221](https://github.com/illegalstudio/elephc/blob/main/src/http_build_query_prelude.rs#L221) (`http_build_query`)
- **Function symbol**: `http_build_query()`


### Lowering notes

- Implemented by the compiler-injected http_build_query prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function http_build_query(array|object $data, string $numeric_prefix = '', ?string $arg_separator = null, int $encoding_type = PHP_QUERY_RFC1738): string
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/http_build_query.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/http_build_query.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `http_build_query()`](../../../php/builtins/string/http_build_query.md)
