---
title: "curl_share_errno()"
description: "Returns the last share curl error number."
---

## curl_share_errno()

```php
function curl_share_errno(CurlShareHandle $share_handle): int
```

Returns the last share curl error number.

**Parameters**:
- `$share_handle` (`CurlShareHandle`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_share_errno` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_share_errno.md).
