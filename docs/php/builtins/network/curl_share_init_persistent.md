---
title: "curl_share_init_persistent()"
description: "Initializes a persistent cURL share handle."
sidebar:
  order: 387
---

## curl_share_init_persistent()

```php
function curl_share_init_persistent(array $share_options): CurlSharePersistentHandle
```

Initializes a persistent cURL share handle.

**Parameters**:
- `$share_options` (`array`)

**Returns**: `CurlSharePersistentHandle`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_share_init_persistent.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_share_init_persistent.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_share_init_persistent` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_share_init_persistent.md).
