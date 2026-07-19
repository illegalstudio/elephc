---
title: "is_array()"
description: "Checks whether a variable is an array."
sidebar:
  order: 425
---

## is_array()

```php
function is_array(mixed $value): bool
```

Checks whether a variable is an array.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_array.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_array.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_array.md).

