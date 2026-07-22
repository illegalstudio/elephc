---
title: "is_float()"
description: "Checks whether a variable is a floating-point number."
sidebar:
  order: 444
---

## is_float()

```php
function is_float(mixed $value): bool
```

Checks whether a variable is a floating-point number.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_float.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_float.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_float` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_float.md).
