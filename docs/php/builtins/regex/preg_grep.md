---
title: "preg_grep()"
description: "Returns the entries of `$array` whose string value matches `$pattern`, keeping their keys; `PREG_GREP_INVERT` keeps the entries that do not match. Returns `false` for an invalid pattern."
---

## preg_grep()

```php
function preg_grep(string $pattern, array $array, int $flags = 0): array|false
```

Returns the entries of `$array` whose string value matches `$pattern`, keeping their keys; `PREG_GREP_INVERT` keeps the entries that do not match. Returns `false` for an invalid pattern.

**Parameters**:
- `$pattern` (`string`)
- `$array` (`array`)
- `$flags` (`int`), default `0`, optional

**Returns**: `array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_grep` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_grep.md).
