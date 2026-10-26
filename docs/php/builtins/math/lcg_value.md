---
title: "lcg_value()"
description: "Combined linear congruential generator."
sidebar:
  order: 587
---

## lcg_value()

```php
function lcg_value(): float
```

Combined linear congruential generator.

**Parameters**: none.

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected random prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `lcg_value` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/lcg_value.md).
