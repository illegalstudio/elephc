---
title: "fflush()"
description: "Lowers `fflush(stream)` through the shared fd flush runtime helper."
sidebar:
  order: 140
---

## fflush()

```php
function fflush(resource $stream): bool
```

Lowers `fflush(stream)` through the shared fd flush runtime helper.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fflush` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fflush.md).

