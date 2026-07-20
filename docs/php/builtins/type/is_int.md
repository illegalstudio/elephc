---
title: "is_int()"
description: "Checks whether a variable is an integer."
sidebar:
  order: 442
---

## is_int()

```php
function is_int(mixed $value): bool
```

Checks whether a variable is an integer.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_int.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_int.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_int` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_int.md).

