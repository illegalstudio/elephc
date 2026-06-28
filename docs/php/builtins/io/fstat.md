---
title: "fstat()"
description: "Lowers `fstat(stream)` and boxes the runtime stat array or PHP false result."
sidebar:
  order: 155
---

## fstat()

```php
function fstat(resource $stream): mixed
```

Lowers `fstat(stream)` and boxes the runtime stat array or PHP false result.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fstat` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fstat.md).

