---
title: "hexdec()"
description: "Converts a hexadecimal string to its decimal number."
sidebar:
  order: 306
---

## hexdec()

```php
function hexdec(string $hex_string): mixed
```

Converts a hexadecimal string to its decimal number.

**Parameters**:
- `$hex_string` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hexdec` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/hexdec.md).
