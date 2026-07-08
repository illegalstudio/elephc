---
title: "sscanf()"
description: "Parses a string according to a format."
sidebar:
  order: 380
---

## sscanf()

```php
function sscanf(string $string, string $format, ...$vars): array
```

Parses a string according to a format.

**Parameters**:
- `$string` (`string`)
- `$format` (`string`)
- `...$vars` — variadic: collects excess arguments into `$vars`.

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sscanf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sscanf.md).

