---
title: "curl_multi_info_read()"
description: "Gets information about the current transfers."
---

## curl_multi_info_read()

```php
function curl_multi_info_read(CurlMultiHandle $multi_handle, ?int $queued_messages = null): mixed
```

Gets information about the current transfers.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)
- `$queued_messages` (`?int`), passed by reference, default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_info_read` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_info_read.md).
