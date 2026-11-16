---
title: "mb_ord()"
description: "Returns the first Unicode codepoint, or false for malformed input."
---

## mb_ord()

```php
function mb_ord(string $string, ?string $encoding = null): int|false
```

Returns the first Unicode codepoint, or false for malformed input.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ord` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ord.md).
