---
title: "decoct()"
description: "Converts an integer to its octal string representation."
sidebar:
  order: 289
---

## decoct()

```php
function decoct(int $num): string
```

Converts an integer to its octal string representation.

**Parameters**:
- `$num` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `decoct` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/decoct.md).
