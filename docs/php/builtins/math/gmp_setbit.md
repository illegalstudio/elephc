---
title: "gmp_setbit()"
description: "Sets or clears a bit in a GMP number."
sidebar:
  order: 632
---

## gmp_setbit()

```php
function gmp_setbit(object $num, int $index, bool $value = true): void
```

Sets or clears a bit in a GMP number.

**Parameters**:
- `$num` (`object`)
- `$index` (`int`)
- `$value` (`bool`), default `true`, optional

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_setbit.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_setbit.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_setbit` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_setbit.md).
