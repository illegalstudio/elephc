---
title: "gmp_perfect_power()"
description: "Checks whether a GMP number is a perfect power."
sidebar:
  order: 619
---

## gmp_perfect_power()

```php
function gmp_perfect_power(object|string|int $num): bool
```

Checks whether a GMP number is a perfect power.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_perfect_power.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_perfect_power.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_perfect_power` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_perfect_power.md).
