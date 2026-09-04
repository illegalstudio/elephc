---
title: "curl_close()"
description: "Closes a cURL session."
sidebar:
  order: 360
---

## curl_close()

```php
function curl_close(CurlHandle $handle): void
```

Closes a cURL session.

**Parameters**:
- `$handle` (`CurlHandle`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_close.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_close.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_close.md).
