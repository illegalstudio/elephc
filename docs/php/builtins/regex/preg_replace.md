---
title: "preg_replace()"
description: "Performs a regular expression search and replace."
---

## preg_replace()

```php
function preg_replace(string $pattern, string $replacement, string $subject): mixed
```

Performs a regular expression search and replace.

**Parameters**:
- `$pattern` (`string`)
- `$replacement` (`string`)
- `$subject` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_replace.md).
