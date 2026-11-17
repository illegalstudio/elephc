---
title: "mb_strlen()"
description: "Returns the character count of a string in the requested encoding."
---

## mb_strlen()

```php
function mb_strlen(string $string, ?string $encoding = null): int
```

Returns the character count of a string in the requested encoding.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strlen` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strlen.md).
