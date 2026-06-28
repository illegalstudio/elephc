---
title: "is_readable()"
description: "Lowers `is_readable(path)` through the target-aware runtime access helper."
sidebar:
  order: 109
---

## is_readable()

```php
function is_readable(string $filename): bool
```

Lowers `is_readable(path)` through the target-aware runtime access helper.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_readable` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_readable.md).

