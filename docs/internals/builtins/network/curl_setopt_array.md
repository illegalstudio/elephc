---
title: "curl_setopt_array() — internals"
description: "Compiler internals for curl_setopt_array(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 383
---

## `curl_setopt_array()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1111](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1111) (`curl_setopt_array`)
- **Function symbol**: `curl_setopt_array()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_setopt_array(CurlHandle $handle, array $options): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_setopt_array.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_setopt_array.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `curl_setopt_array()`](../../../php/builtins/network/curl_setopt_array.md)
