---
title: "bindec()"
description: "Converts a binary string to its decimal number."
sidebar:
  order: 307
---

## bindec()

```php
function bindec(string $binary_string): mixed
```

Converts a binary string to its decimal number.

**Parameters**:
- `$binary_string` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bindec` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bindec.md).
