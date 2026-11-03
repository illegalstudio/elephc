---
title: "preg_match()"
description: "Performs a regular expression match."
---

## preg_match()

```php
function preg_match(string $pattern, string $subject, array $matches = null): int|false
```

Performs a regular expression match.

**Parameters**:
- `$pattern` (`string`)
- `$subject` (`string`)
- `$matches` (`array`), passed by reference, default `null`, optional

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_match` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_match.md).
