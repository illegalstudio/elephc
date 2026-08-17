---
title: "curl_copy_handle() — internals"
description: "Compiler internals for curl_copy_handle(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 335
---

## `curl_copy_handle()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1307](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1307) (`curl_copy_handle`)
- **Function symbol**: `curl_copy_handle()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_copy_handle(CurlHandle $handle): CurlHandle
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_copy_handle.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_copy_handle.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `curl_copy_handle()`](../../../php/builtins/network/curl_copy_handle.md)
