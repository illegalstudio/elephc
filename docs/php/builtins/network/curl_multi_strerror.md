---
title: "curl_multi_strerror()"
description: "Returns string describing error code."
---

## curl_multi_strerror()

```php
function curl_multi_strerror(int $error_code): string
```

Returns string describing error code.

**Parameters**:
- `$error_code` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_strerror` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_strerror.md).
