---
title: "bcpow()"
description: "Raises an arbitrary-precision decimal number to an integral power."
sidebar:
  order: 287
---

## bcpow()

```php
function bcpow(string $num, string $exponent, int $scale = null): string
```

Raises an arbitrary-precision decimal number to an integral power.

**Parameters**:
- `$num` (`string`)
- `$exponent` (`string`)
- `$scale` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcpow.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcpow.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcpow` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcpow.md).
