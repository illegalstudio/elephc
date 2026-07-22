---
title: "array_replace()"
description: "Replaces elements from passed arrays into the first array."
sidebar:
  order: 32
---

## array_replace()

```php
function array_replace(array $array, array $replacements): mixed
```

Replaces elements from passed arrays into the first array.

**Parameters**:
- `$array` (`array`)
- `$replacements` (`array`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_replace.md).
