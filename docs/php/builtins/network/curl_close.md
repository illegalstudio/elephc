---
title: "curl_close()"
description: "Closes a cURL session."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_close.md).
