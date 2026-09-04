---
title: "curl_multi_add_handle() — internals"
description: "Compiler internals for curl_multi_add_handle(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 368
---

## `curl_multi_add_handle()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1529](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1529) (`curl_multi_add_handle`)
- **Function symbol**: `curl_multi_add_handle()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_multi_add_handle(CurlMultiHandle $multi_handle, mixed $handle): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_add_handle.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_add_handle.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `curl_multi_add_handle()`](../../../php/builtins/network/curl_multi_add_handle.md)
