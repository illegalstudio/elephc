---
title: "rewind()"
description: "Lowers `rewind(stream)` as `lseek(fd, 0, SEEK_SET)` and clears EOF state on success."
sidebar:
  order: 170
---

## rewind()

```php
function rewind(resource $stream): bool
```

Lowers `rewind(stream)` as `lseek(fd, 0, SEEK_SET)` and clears EOF state on success.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rewind` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/rewind.md).

