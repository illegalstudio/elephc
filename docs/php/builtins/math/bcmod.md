---
title: "bcmod()"
description: "Returns the remainder of arbitrary-precision decimal division."
sidebar:
  order: 299
---

## bcmod()

```php
function bcmod(string $num1, string $num2, int $scale = null): string
```

Returns the remainder of arbitrary-precision decimal division.

**Parameters**:
- `$num1` (`string`)
- `$num2` (`string`)
- `$scale` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcmod.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcmod.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcmod` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcmod.md).
