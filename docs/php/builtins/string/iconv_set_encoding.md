---
title: "iconv_set_encoding()"
description: "Sets the input, output, or internal character encoding."
---

## iconv_set_encoding()

```php
function iconv_set_encoding(string $type, string $encoding): bool
```

Sets the input, output, or internal character encoding.

**Parameters**:
- `$type` (`string`)
- `$encoding` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_set_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_set_encoding.md).
