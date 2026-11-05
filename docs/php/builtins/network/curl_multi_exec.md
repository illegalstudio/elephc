---
title: "curl_multi_exec()"
description: "Runs the sub-connections of the current cURL handle."
---

## curl_multi_exec()

```php
function curl_multi_exec(CurlMultiHandle $multi_handle, int &$still_running): int
```

Runs the sub-connections of the current cURL handle.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)
- `$still_running` (`int`), passed by reference

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_exec` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_exec.md).
