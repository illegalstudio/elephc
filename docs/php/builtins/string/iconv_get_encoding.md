---
title: "iconv_get_encoding()"
description: "Reports the configured input, output, or internal character encoding."
---

## iconv_get_encoding()

```php
function iconv_get_encoding(string $type = 'all'): mixed
```

Reports the configured input, output, or internal character encoding.

**Parameters**:
- `$type` (`string`), default `'all'`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_get_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_get_encoding.md).
