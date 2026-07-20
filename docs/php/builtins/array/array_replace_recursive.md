---
title: "array_replace_recursive()"
description: "Replaces elements from passed arrays into the first array recursively."
sidebar:
  order: 33
---

## array_replace_recursive()

```php
function array_replace_recursive(array $array, array $replacements): mixed
```

Replaces elements from passed arrays into the first array recursively.

**Parameters**:
- `$array` (`array`)
- `$replacements` (`array`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_replace_recursive` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_replace_recursive.md).

