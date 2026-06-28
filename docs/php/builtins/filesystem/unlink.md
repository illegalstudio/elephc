---
title: "unlink()"
description: "Lowers `unlink(path)` through the target-aware runtime helper."
sidebar:
  order: 135
---

## unlink()

```php
function unlink(string $filename): bool
```

Lowers `unlink(path)` through the target-aware runtime helper.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `unlink` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/unlink.md).

