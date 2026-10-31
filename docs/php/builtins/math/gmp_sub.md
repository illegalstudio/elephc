---
title: "gmp_sub()"
description: "Subtracts two GMP numbers."
sidebar:
  order: 637
---

## gmp_sub()

```php
function gmp_sub(object|string|int $num1, object|string|int $num2): object
```

Subtracts two GMP numbers.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_sub.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_sub.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_sub` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_sub.md).
