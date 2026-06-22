---
title: "ptr_is_null()"
description: "Lowers `ptr_is_null(pointer)` by comparing the raw pointer address to zero."
sidebar:
  order: 272
---

## ptr_is_null()

```php
function ptr_is_null(pointer $pointer): bool
```

Lowers `ptr_is_null(pointer)` by comparing the raw pointer address to zero.

**Parameters**:
- `$pointer` (`pointer`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_is_null` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_is_null.md).

