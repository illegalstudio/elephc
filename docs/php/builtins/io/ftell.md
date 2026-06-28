---
title: "ftell()"
description: "Lowers `ftell(stream)` as `lseek(fd, 0, SEEK_CUR)`."
sidebar:
  order: 157
---

## ftell()

```php
function ftell(resource $stream): int
```

Lowers `ftell(stream)` as `lseek(fd, 0, SEEK_CUR)`.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ftell` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ftell.md).

