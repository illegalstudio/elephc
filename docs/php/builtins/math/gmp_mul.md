---
title: "gmp_mul()"
description: "Multiplies two GMP numbers."
sidebar:
  order: 615
---

## gmp_mul()

```php
function gmp_mul(object|string|int $num1, object|string|int $num2): object
```

Multiplies two GMP numbers.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_mul.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_mul.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_mul` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_mul.md).
