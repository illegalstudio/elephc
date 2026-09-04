---
title: "curl_share_close()"
description: "Closes a cURL share handle."
sidebar:
  order: 384
---

## curl_share_close()

```php
function curl_share_close(CurlShareHandle $share_handle): void
```

Closes a cURL share handle.

**Parameters**:
- `$share_handle` (`CurlShareHandle`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_share_close.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_share_close.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_share_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_share_close.md).
