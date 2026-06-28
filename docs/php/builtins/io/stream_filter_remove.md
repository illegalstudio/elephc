---
title: "stream_filter_remove()"
description: "Lowers `stream_filter_remove(filter)` and clears both direction tables for the fd."
sidebar:
  order: 183
---

## stream_filter_remove()

```php
function stream_filter_remove(resource $stream_filter): bool
```

Lowers `stream_filter_remove(filter)` and clears both direction tables for the fd.

**Parameters**:
- `$stream_filter` (`resource`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_filter_remove` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_filter_remove.md).

