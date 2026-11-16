---
title: "curl_errno()"
description: "Returns the last error number for a cURL session."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_errno` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_errno.md).
