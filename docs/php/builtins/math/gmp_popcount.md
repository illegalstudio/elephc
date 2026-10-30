---
title: "gmp_popcount()"
description: "Population count of a GMP number."
sidebar:
  order: 621
---

## gmp_popcount()

```php
function gmp_popcount(object|string|int $num): int
```

Population count of a GMP number.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_popcount.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_popcount.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_popcount` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_popcount.md).
