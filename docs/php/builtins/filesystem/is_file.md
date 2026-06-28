---
title: "is_file()"
description: "Lowers `is_file(path)` through the target-aware runtime stat helper."
sidebar:
  order: 107
---

## is_file()

```php
function is_file(string $filename): bool
```

Lowers `is_file(path)` through the target-aware runtime stat helper.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_file.md).

