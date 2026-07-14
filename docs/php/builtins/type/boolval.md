---
title: "boolval()"
description: "Returns the boolean value of a variable."
sidebar:
  order: 414
---

## boolval()

```php
function boolval(mixed $value): bool
```

Returns the boolean value of a variable.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/boolval.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/boolval.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `boolval` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/boolval.md).

