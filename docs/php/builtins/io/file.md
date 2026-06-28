---
title: "file()"
description: "Lowers `file(path)` through the target-aware runtime line-array helper."
sidebar:
  order: 144
---

## file()

```php
function file(string $filename, int $flags, mixed $context): array
```

Lowers `file(path)` through the target-aware runtime line-array helper.

**Parameters**:
- `$filename` (`string`)
- `$flags` (`int`)
- `$context` (`mixed`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `file` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/file.md).

