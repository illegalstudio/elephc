---
title: "natcasesort()"
description: "Sorts an array using a case-insensitive natural order algorithm."
sidebar:
  order: 55
---

## natcasesort()

```php
function natcasesort(array $array): bool
```

Sorts an array using a case-insensitive natural order algorithm.

**Parameters**:
- `$array` (`array`), passed by reference

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/natcasesort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/natcasesort.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `natcasesort` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/natcasesort.md).

