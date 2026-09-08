---
title: "sizeof()"
description: "Alias of count."
sidebar:
  order: 67
---

## sizeof()

```php
function sizeof(mixed $value, int $mode = 0): int
```

Alias of count.

**Parameters**:
- `$value` (`mixed`)
- `$mode` (`int`), default `0`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sizeof` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/sizeof.md).
