---
title: "is_link()"
description: "Lowers `is_link(path)` through the target-aware runtime lstat helper."
sidebar:
  order: 108
---

## is_link()

```php
function is_link(string $filename): bool
```

Lowers `is_link(path)` through the target-aware runtime lstat helper.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_link` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_link.md).

