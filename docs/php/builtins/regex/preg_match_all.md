---
title: "preg_match_all()"
description: "Performs a global regular expression match, including terminal empty matches, optionally filling `$matches`, and returns the number of matches. AOT capture output supports local variables and local references; unsupported non-local destinations are diagnosed. Unsupported flag bits or conflicting order modes raise PHP's `ValueError` on both backends."
---

## preg_match_all()

```php
function preg_match_all(string $pattern, string $subject, mixed $matches = null, int $flags = 0): int|false
```

Performs a global regular expression match, including terminal empty matches, optionally filling `$matches`, and returns the number of matches. AOT capture output supports local variables and local references; unsupported non-local destinations are diagnosed. Unsupported flag bits or conflicting order modes raise PHP's `ValueError` on both backends.

**Parameters**:
- `$pattern` (`string`)
- `$subject` (`string`)
- `$matches` (`mixed`), passed by reference, default `null`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_match_all` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_match_all.md).
