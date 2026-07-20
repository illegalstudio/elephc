---
title: "is_numeric()"
description: "Checks whether a variable is a number or a numeric string."
sidebar:
  order: 445
---

## is_numeric()

```php
function is_numeric(mixed $value): bool
```

Checks whether a variable is a number or a numeric string.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_numeric.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_numeric.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_numeric` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_numeric.md).

