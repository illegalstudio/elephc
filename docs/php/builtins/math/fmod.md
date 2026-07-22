---
title: "fmod()"
description: "Returns the floating point remainder of the division of the arguments."
sidebar:
  order: 267
---

## fmod()

```php
function fmod(float $num1, float $num2): float
```

Returns the floating point remainder of the division of the arguments.

**Parameters**:
- `$num1` (`float`)
- `$num2` (`float`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/fmod.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/fmod.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fmod` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/fmod.md).
