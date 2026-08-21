---
title: "decbin()"
description: "Converts an integer to its binary string representation."
sidebar:
  order: 298
---

## decbin()

```php
function decbin(int $num): string
```

Converts an integer to its binary string representation.

**Parameters**:
- `$num` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `decbin` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/decbin.md).
