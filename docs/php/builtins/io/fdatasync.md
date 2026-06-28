---
title: "fdatasync()"
description: "Lowers `fdatasync(stream)` through the shared fd data-sync runtime helper."
sidebar:
  order: 138
---

## fdatasync()

```php
function fdatasync(resource $stream): bool
```

Lowers `fdatasync(stream)` through the shared fd data-sync runtime helper.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fdatasync` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fdatasync.md).

