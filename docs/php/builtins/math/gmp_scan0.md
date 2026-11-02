---
title: "gmp_scan0()"
description: "Finds the first zero bit."
sidebar:
  order: 630
---

## gmp_scan0()

```php
function gmp_scan0(object|string|int $num1, int $start): int
```

Finds the first zero bit.

**Parameters**:
- `$num1` (`object|string|int`)
- `$start` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_scan0.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_scan0.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_scan0` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_scan0.md).
