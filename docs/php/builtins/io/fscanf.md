---
title: "fscanf()"
description: "Lowers `fscanf(stream, format)` through `__rt_fgets` and `__rt_sscanf`."
sidebar:
  order: 153
---

## fscanf()

```php
function fscanf(resource $stream, string $format, ...$vars): array
```

Lowers `fscanf(stream, format)` through `__rt_fgets` and `__rt_sscanf`.

**Parameters**:
- `$stream` (`resource`)
- `$format` (`string`)
- `...$vars` — variadic: collects excess arguments into `$vars`.

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fscanf` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fscanf.md).

