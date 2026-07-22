---
title: "fdiv()"
description: "Divides two numbers, according to IEEE 754."
sidebar:
  order: 265
---

## fdiv()

```php
function fdiv(float $num1, float $num2): float
```

Divides two numbers, according to IEEE 754.

**Parameters**:
- `$num1` (`float`)
- `$num2` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/fdiv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/fdiv.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fdiv` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/fdiv.md).
