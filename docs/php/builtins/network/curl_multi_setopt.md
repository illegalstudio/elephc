---
title: "curl_multi_setopt()"
description: "Sets an option on a cURL multi handle."
---

## curl_multi_setopt()

```php
function curl_multi_setopt(CurlMultiHandle $multi_handle, int $option, mixed $value): bool
```

Sets an option on a cURL multi handle.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)
- `$option` (`int`)
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_setopt` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_setopt.md).
