---
title: "curl_copy_handle()"
description: "Copies a cURL handle along with all of its preferences."
---

## curl_copy_handle()

```php
function curl_copy_handle(CurlHandle $handle): CurlHandle
```

Copies a cURL handle along with all of its preferences.

**Parameters**:
- `$handle` (`CurlHandle`)

**Returns**: `CurlHandle`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_copy_handle` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_copy_handle.md).
