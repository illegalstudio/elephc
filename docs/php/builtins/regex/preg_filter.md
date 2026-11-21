---
title: "preg_filter()"
description: "Performs `preg_replace()` but returns `null` when `$pattern` matches nothing in `$subject` or is invalid."
---

## preg_filter()

```php
function preg_filter(string $pattern, string $replacement, string $subject): ?string
```

Performs `preg_replace()` but returns `null` when `$pattern` matches nothing in `$subject` or is invalid.

**Parameters**:
- `$pattern` (`string`)
- `$replacement` (`string`)
- `$subject` (`string`)

**Returns**: `?string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_filter` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_filter.md).
