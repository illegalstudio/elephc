---
title: "mb_regex_encoding()"
description: "Reads or changes the multibyte regex encoding independently of the internal text encoding."
---

## mb_regex_encoding()

```php
function mb_regex_encoding(?string $encoding = null): string|bool
```

Reads or changes the multibyte regex encoding independently of the internal text encoding.

**Parameters**:
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string|bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_regex_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_regex_encoding.md).
