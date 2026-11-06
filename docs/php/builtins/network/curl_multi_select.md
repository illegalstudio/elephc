---
title: "curl_multi_select()"
description: "Waits until there is activity on any cURL multi connection."
---

## curl_multi_select()

```php
function curl_multi_select(CurlMultiHandle $multi_handle, float $timeout = 1.0): int
```

Waits until there is activity on any cURL multi connection.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)
- `$timeout` (`float`), default `1.0`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_select` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_select.md).
