---
title: "vfprintf()"
description: "Lowers `vfprintf(stream, format, values)` through `__rt_vsprintf` then fwrite."
sidebar:
  order: 212
---

## vfprintf()

```php
function vfprintf(resource $stream, string $format, array $values): int
```

Lowers `vfprintf(stream, format, values)` through `__rt_vsprintf` then fwrite.

**Parameters**:
- `$stream` (`resource`)
- `$format` (`string`)
- `$values` (`array`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `vfprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/vfprintf.md).

