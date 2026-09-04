---
title: "curl_getinfo() — internals"
description: "Compiler internals for curl_getinfo(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 366
---

## `curl_getinfo()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1177](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1177) (`curl_getinfo`)
- **Function symbol**: `curl_getinfo()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_getinfo(CurlHandle $handle, int $option = null): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_getinfo.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_getinfo.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `curl_getinfo()`](../../../php/builtins/network/curl_getinfo.md)
