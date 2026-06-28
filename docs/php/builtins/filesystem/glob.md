---
title: "glob()"
description: "Lowers `glob(pattern)` through the target-aware runtime glob expansion helper."
sidebar:
  order: 104
---

## glob()

```php
function glob(string $pattern, int $flags): array
```

Lowers `glob(pattern)` through the target-aware runtime glob expansion helper.

**Parameters**:
- `$pattern` (`string`)
- `$flags` (`int`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `glob` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/glob.md).

