---
title: "floatval()"
description: "Returns the float value of a variable."
sidebar:
  order: 420
---

## floatval()

```php
function floatval(mixed $value): float
```

Returns the float value of a variable.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/floatval.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/floatval.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `floatval` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/floatval.md).

