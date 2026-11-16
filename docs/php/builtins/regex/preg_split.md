---
title: "preg_split()"
description: "Splits a string by a regular expression."
---

## preg_split()

```php
function preg_split(string $pattern, string $subject, int $limit = -1, int $flags = 0): array|false
```

Splits a string by a regular expression.

**Parameters**:
- `$pattern` (`string`)
- `$subject` (`string`)
- `$limit` (`int`), default `-1`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_split` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_split.md).
