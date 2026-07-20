---
title: "is_null()"
description: "Checks whether a variable is null."
sidebar:
  order: 444
---

## is_null()

```php
function is_null(mixed $value): bool
```

Checks whether a variable is null.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_null.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_null.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_null` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_null.md).

