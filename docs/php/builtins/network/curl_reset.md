---
title: "curl_reset()"
description: "Resets all options of a libcurl session handle."
---

## curl_reset()

```php
function curl_reset(CurlHandle $handle): void
```

Resets all options of a libcurl session handle.

**Parameters**:
- `$handle` (`CurlHandle`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_reset` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_reset.md).
