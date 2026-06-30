---
title: "ptr_set()"
description: "Lowers `ptr_set(pointer, value)` by writing one machine word through a checked pointer."
sidebar:
  order: 294
---

## ptr_set()

```php
function ptr_set(pointer $pointer, mixed $value): void
```

Lowers `ptr_set(pointer, value)` by writing one machine word through a checked pointer.

**Parameters**:
- `$pointer` (`pointer`)
- `$value` (`mixed`)

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_set.md).

