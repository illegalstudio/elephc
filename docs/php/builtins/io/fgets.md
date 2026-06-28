---
title: "fgets()"
description: "Lowers `fgets(stream)` through the shared line-read runtime helper."
sidebar:
  order: 143
---

## fgets()

```php
function fgets(resource $stream, int $length): mixed
```

Lowers `fgets(stream)` through the shared line-read runtime helper.

**Parameters**:
- `$stream` (`resource`)
- `$length` (`int`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fgets` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fgets.md).

