---
title: "curl_escape() — internals"
description: "Compiler internals for curl_escape(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 338
---

## `curl_escape()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1379](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1379) (`curl_escape`)
- **Function symbol**: `curl_escape()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_escape(CurlHandle $handle, string $string): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_escape.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_escape.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `curl_escape()`](../../../php/builtins/network/curl_escape.md)
