---
title: "is_writable()"
description: "Lowers `is_writable(path)` through the target-aware runtime access helper."
sidebar:
  order: 110
---

## is_writable()

```php
function is_writable(string $filename): bool
```

Lowers `is_writable(path)` through the target-aware runtime access helper.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_writable` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_writable.md).

