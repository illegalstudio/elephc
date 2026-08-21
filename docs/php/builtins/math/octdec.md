---
title: "octdec()"
description: "Converts a octal string to its decimal number."
sidebar:
  order: 318
---

## octdec()

```php
function octdec(string $octal_string): mixed
```

Converts a octal string to its decimal number.

**Parameters**:
- `$octal_string` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `octdec` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/octdec.md).
