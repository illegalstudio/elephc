---
title: "gmp_testbit()"
description: "Tests a bit in a GMP number."
sidebar:
  order: 638
---

## gmp_testbit()

```php
function gmp_testbit(object|string|int $num, int $index): bool
```

Tests a bit in a GMP number.

**Parameters**:
- `$num` (`object|string|int`)
- `$index` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_testbit.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_testbit.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_testbit` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_testbit.md).
