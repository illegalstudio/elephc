---
title: "iconv_strlen()"
description: "Returns the character count of a string in the requested encoding."
---

## iconv_strlen()

```php
function iconv_strlen(string $string, ?string $encoding = null): mixed
```

Returns the character count of a string in the requested encoding.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_strlen` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_strlen.md).
