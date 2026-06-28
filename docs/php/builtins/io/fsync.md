---
title: "fsync()"
description: "Lowers `fsync(stream)` through the shared fd sync runtime helper."
sidebar:
  order: 156
---

## fsync()

```php
function fsync(resource $stream): bool
```

Lowers `fsync(stream)` through the shared fd sync runtime helper.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fsync` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fsync.md).

