---
title: "preg_replace_callback()"
description: "Performs a regular expression search and replace using a callback."
---

## preg_replace_callback()

```php
function preg_replace_callback(string $pattern, callable $callback, string $subject): mixed
```

Performs a regular expression search and replace using a callback.

**Parameters**:
- `$pattern` (`string`)
- `$callback` (`callable`)
- `$subject` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_replace_callback` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_replace_callback.md).
