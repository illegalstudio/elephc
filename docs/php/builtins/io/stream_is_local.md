---
title: "stream_is_local()"
description: "Lowers `stream_is_local(stream)` as a true predicate after evaluating its argument."
sidebar:
  order: 190
---

## stream_is_local()

```php
function stream_is_local(resource $stream): bool
```

Lowers `stream_is_local(stream)` as a true predicate after evaluating its argument.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_is_local` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_is_local.md).

