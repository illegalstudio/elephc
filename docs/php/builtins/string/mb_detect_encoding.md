---
title: "mb_detect_encoding()"
description: "Guesses the most likely candidate encoding using the request's detection settings."
---

## mb_detect_encoding()

```php
function mb_detect_encoding(string $string, array|string|null $encodings = null, bool $strict = false): string|false
```

Guesses the most likely candidate encoding using the request's detection settings.

**Parameters**:
- `$string` (`string`)
- `$encodings` (`array|string|null`), default `null`, optional
- `$strict` (`bool`), default `false`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_detect_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_detect_encoding.md).
