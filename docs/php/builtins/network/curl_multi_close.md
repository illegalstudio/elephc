---
title: "curl_multi_close()"
description: "Closes a set of cURL handles."
---

## curl_multi_close()

```php
function curl_multi_close(CurlMultiHandle $multi_handle): void
```

Closes a set of cURL handles.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_close.md).
