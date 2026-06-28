---
title: "scandir()"
description: "Lowers `scandir(path)` through the target-aware runtime directory listing helper."
sidebar:
  order: 127
---

## scandir()

```php
function scandir(string $directory, int $sorting_order, mixed $context): array
```

Lowers `scandir(path)` through the target-aware runtime directory listing helper.

**Parameters**:
- `$directory` (`string`)
- `$sorting_order` (`int`)
- `$context` (`mixed`)

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `scandir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/scandir.md).

