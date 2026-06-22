---
title: "ptr_get()"
description: "Lowers `ptr_get(pointer)` by reading one machine word through a checked pointer."
sidebar:
  order: 271
---

## ptr_get()

```php
function ptr_get(pointer $pointer): int
```

Lowers `ptr_get(pointer)` by reading one machine word through a checked pointer.

**Parameters**:
- `$pointer` (`pointer`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_get.md).

