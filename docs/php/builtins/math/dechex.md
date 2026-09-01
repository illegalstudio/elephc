---
title: "dechex()"
description: "Converts an integer to its hexadecimal string representation."
sidebar:
  order: 288
---

## dechex()

```php
function dechex(int $num): string
```

Converts an integer to its hexadecimal string representation.

**Parameters**:
- `$num` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `dechex` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/dechex.md).
