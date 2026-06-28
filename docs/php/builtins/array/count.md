---
title: "count()"
description: "Lowers `count(array)` for concrete array values by reading the runtime length header."
sidebar:
  order: 34
---

## count()

```php
function count(array $value, int $mode): int
```

Lowers `count(array)` for concrete array values by reading the runtime length header.

**Parameters**:
- `$value` (`array`)
- `$mode` (`int`), optional

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `count` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/count.md).

