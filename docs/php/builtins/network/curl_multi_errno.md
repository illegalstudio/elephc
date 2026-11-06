---
title: "curl_multi_errno()"
description: "Returns the last multi curl error number."
---

## curl_multi_errno()

```php
function curl_multi_errno(CurlMultiHandle $multi_handle): int
```

Returns the last multi curl error number.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_errno` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_errno.md).
