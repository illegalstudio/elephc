---
title: "getrandmax()"
description: "Returns the largest possible random value."
sidebar:
  order: 295
---

## getrandmax()

```php
function getrandmax(): int
```

Returns the largest possible random value.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `getrandmax` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/getrandmax.md).
