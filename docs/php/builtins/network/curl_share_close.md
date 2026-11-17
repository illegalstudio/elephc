---
title: "curl_share_close()"
description: "Closes a cURL share handle."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_share_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_share_close.md).
