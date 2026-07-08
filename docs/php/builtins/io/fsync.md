---
title: "fsync()"
description: "Synchronizes changes to the file (including meta-data)."
sidebar:
  order: 173
---

## fsync()

```php
function fsync(resource $stream): bool
```

Synchronizes changes to the file (including meta-data).

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fsync` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fsync.md).

