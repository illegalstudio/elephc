---
title: "bcpowmod()"
description: "Returns an arbitrary-precision integral modular power."
sidebar:
  order: 302
---

## bcpowmod()

```php
function bcpowmod(string $num, string $exponent, string $modulus, int $scale = null): string
```

Returns an arbitrary-precision integral modular power.

**Parameters**:
- `$num` (`string`)
- `$exponent` (`string`)
- `$modulus` (`string`)
- `$scale` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcpowmod.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcpowmod.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcpowmod` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcpowmod.md).
