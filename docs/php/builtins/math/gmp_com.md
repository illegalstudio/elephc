---
title: "gmp_com()"
description: "One's complement of a GMP number."
sidebar:
  order: 595
---

## gmp_com()

```php
function gmp_com(object|string|int $num): object
```

One's complement of a GMP number.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_com.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_com.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_com` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_com.md).
