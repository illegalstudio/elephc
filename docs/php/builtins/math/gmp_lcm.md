---
title: "gmp_lcm()"
description: "Least common multiple."
sidebar:
  order: 612
---

## gmp_lcm()

```php
function gmp_lcm(object|string|int $num1, object|string|int $num2): object
```

Least common multiple.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_lcm.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_lcm.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_lcm` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_lcm.md).
