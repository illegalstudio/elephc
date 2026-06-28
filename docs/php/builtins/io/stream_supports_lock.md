---
title: "stream_supports_lock()"
description: "Lowers `stream_supports_lock(stream)` as true after resource unboxing."
sidebar:
  order: 208
---

## stream_supports_lock()

```php
function stream_supports_lock(resource $stream): bool
```

Lowers `stream_supports_lock(stream)` as true after resource unboxing.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_supports_lock` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_supports_lock.md).

