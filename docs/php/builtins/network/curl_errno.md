---
title: "curl_errno()"
description: "Returns the last error number for a cURL session."
sidebar:
  order: 336
---

## curl_errno()

```php
function curl_errno(CurlHandle $handle): int
```

Returns the last error number for a cURL session.

**Parameters**:
- `$handle` (`CurlHandle`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_errno.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_errno.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_errno` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_errno.md).
