---
title: "curl_share_init_persistent()"
description: "Initializes a persistent cURL share handle."
---

## curl_share_init_persistent()

```php
function curl_share_init_persistent(array $share_options): CurlSharePersistentHandle
```

Initializes a persistent cURL share handle.

**Parameters**:
- `$share_options` (`array`)

**Returns**: `CurlSharePersistentHandle`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_share_init_persistent` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_share_init_persistent.md).
