---
title: "sscanf()"
description: "Lowers `sscanf(string, format)` into the shared scanner helper."
sidebar:
  order: 376
---

## sscanf()

```php
function sscanf(string $string, string $format, ...$vars): array
```

Lowers `sscanf(string, format)` into the shared scanner helper.

**Parameters**:
- `$string` (`string`)
- `$format` (`string`)
- `...$vars` — variadic: collects excess arguments into `$vars`.

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sscanf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sscanf.md).

