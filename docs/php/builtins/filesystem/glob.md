---
title: "glob()"
description: "Finds pathnames matching a pattern."
---

## glob()

```php
function glob(string $pattern): array
```

Finds pathnames matching a pattern.

**Parameters**:
- `$pattern` (`string`)

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `glob` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/glob.md).
