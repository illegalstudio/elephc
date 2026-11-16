---
title: "curl_share_setopt()"
description: "Sets an option for a cURL share handle."
---

## curl_share_setopt()

```php
function curl_share_setopt(CurlShareHandle $share_handle, int $option, mixed $value): bool
```

Sets an option for a cURL share handle.

**Parameters**:
- `$share_handle` (`CurlShareHandle`)
- `$option` (`int`)
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_share_setopt` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_share_setopt.md).
