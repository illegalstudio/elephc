---
title: "preg_replace_callback()"
description: "Lowers `preg_replace_callback(pattern, callback, subject)` through supported direct callbacks."
sidebar:
  order: 314
---

## preg_replace_callback()

```php
function preg_replace_callback(string $pattern, callable $callback, string $subject, int $limit = -1, int $count = null, int $flags = 0): array
```

Lowers `preg_replace_callback(pattern, callback, subject)` through supported direct callbacks.

**Parameters**:
- `$pattern` (`string`)
- `$callback` (`callable`)
- `$subject` (`string`)
- `$limit` (`int`), default `-1`, optional
- `$count` (`int`), passed by reference, default `null`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `preg_replace_callback` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_replace_callback.md).

