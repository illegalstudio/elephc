---
title: "umask()"
description: "Lowers `umask(mask?)` through the target-aware runtime helper."
sidebar:
  order: 134
---

## umask()

```php
function umask(int $mask): int
```

Lowers `umask(mask?)` through the target-aware runtime helper.

**Parameters**:
- `$mask` (`int`), optional

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `umask` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/umask.md).

