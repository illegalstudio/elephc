---
title: "gmp_gcdext()"
description: "Extended greatest common divisor."
sidebar:
  order: 604
---

## gmp_gcdext()

```php
function gmp_gcdext(object|string|int $num1, object|string|int $num2): array
```

Extended greatest common divisor.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_gcdext.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_gcdext.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_gcdext` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_gcdext.md).
