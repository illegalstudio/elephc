---
title: "gmp_invert()"
description: "Modular inverse."
sidebar:
  order: 609
---

## gmp_invert()

```php
function gmp_invert(object|string|int $num1, object|string|int $num2): object|false
```

Modular inverse.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `object|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_invert.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_invert.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_invert` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_invert.md).
