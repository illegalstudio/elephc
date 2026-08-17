---
title: "curl_multi_setopt() — internals"
description: "Compiler internals for curl_multi_setopt(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 352
---

## `curl_multi_setopt()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1699](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1699) (`curl_multi_setopt`)
- **Function symbol**: `curl_multi_setopt()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_multi_setopt(CurlMultiHandle $multi_handle, int $option, mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_setopt.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_setopt.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `curl_multi_setopt()`](../../../php/builtins/network/curl_multi_setopt.md)
