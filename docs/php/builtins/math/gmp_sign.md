---
title: "gmp_sign()"
description: "Sign of a GMP number."
sidebar:
  order: 633
---

## gmp_sign()

```php
function gmp_sign(object|string|int $num): int
```

Sign of a GMP number.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_sign.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_sign.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_sign` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_sign.md).
