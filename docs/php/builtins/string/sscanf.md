---
title: "sscanf()"
description: "Parses a string according to a format."
---

## sscanf()

```php
function sscanf(string $string, string $format, ...$vars): array
```

Parses a string according to a format.

**Parameters**:
- `$string` (`string`)
- `$format` (`string`)
- `...$vars` - variadic: collects excess arguments into `$vars`.

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sscanf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sscanf.md).
