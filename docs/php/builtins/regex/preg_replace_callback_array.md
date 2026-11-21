---
title: "preg_replace_callback_array()"
description: "Applies each `pattern => callback` entry of `$pattern` to `$subject` in order, like chained `preg_replace_callback()` calls; returns `null` when an entry fails."
---

## preg_replace_callback_array()

```php
function preg_replace_callback_array(array $pattern, string $subject): ?string
```

Applies each `pattern => callback` entry of `$pattern` to `$subject` in order, like chained `preg_replace_callback()` calls; returns `null` when an entry fails.

**Parameters**:
- `$pattern` (`array`)
- `$subject` (`string`)

**Returns**: `?string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_replace_callback_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_replace_callback_array.md).
