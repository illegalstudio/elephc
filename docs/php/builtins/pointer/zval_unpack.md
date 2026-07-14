---
title: "zval_unpack()"
description: "Unpacks a PHP zval pointer into an owned elephc Mixed value."
sidebar:
  order: 307
---

## zval_unpack()

```php
function zval_unpack(pointer $zval): mixed
```

Unpacks a PHP zval pointer into an owned elephc Mixed value.

**Parameters**:
- `$zval` (`pointer`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `zval_unpack` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_unpack.md).

