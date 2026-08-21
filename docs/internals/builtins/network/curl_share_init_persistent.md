---
title: "curl_share_init_persistent() — internals"
description: "Compiler internals for curl_share_init_persistent(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 361
---

## `curl_share_init_persistent()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1903](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1903) (`curl_share_init_persistent`)
- **Function symbol**: `curl_share_init_persistent()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_share_init_persistent(array $share_options): CurlSharePersistentHandle
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_share_init_persistent.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_share_init_persistent.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `curl_share_init_persistent()`](../../../php/builtins/network/curl_share_init_persistent.md)
