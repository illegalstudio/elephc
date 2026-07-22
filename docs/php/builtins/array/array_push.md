---
title: "array_push()"
description: "Pushes one or more elements onto the end of array."
sidebar:
  order: 29
---

## array_push()

```php
function array_push(array $array, ...$values): void
```

Pushes one or more elements onto the end of array.

**Parameters**:
- `$array` (`array`), passed by reference
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_push.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_push.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_push` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_push.md).
