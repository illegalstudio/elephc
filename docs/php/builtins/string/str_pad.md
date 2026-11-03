---
title: "str_pad()"
description: "Pads a string to a certain length with another string."
---

## str_pad()

```php
function str_pad(string $string, int $length, string $pad_string = ' ', int $pad_type = 1): string
```

Pads a string to a certain length with another string.

**Parameters**:
- `$string` (`string`)
- `$length` (`int`)
- `$pad_string` (`string`), default `' '`, optional
- `$pad_type` (`int`), default `1`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `str_pad` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_pad.md).
