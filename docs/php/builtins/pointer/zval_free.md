---
title: "zval_free()"
description: "Frees a PHP zval pointer allocated by `zval_pack`."
sidebar:
  order: 304
---

## zval_free()

```php
function zval_free(pointer $zval): void
```

Frees a PHP zval pointer allocated by `zval_pack`.

**Parameters**:
- `$zval` (`pointer`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `zval_free` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/zval_free.md).

