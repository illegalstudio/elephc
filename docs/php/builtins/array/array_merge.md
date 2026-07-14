---
title: "array_merge()"
description: "Merges the elements of two arrays."
sidebar:
  order: 23
---

## array_merge()

```php
function array_merge(...$arrays): array
```

Merges the elements of two arrays.

**Parameters**:
- `...$arrays` — variadic: collects excess arguments into `$arrays`.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_merge.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_merge.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_merge` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_merge.md).

