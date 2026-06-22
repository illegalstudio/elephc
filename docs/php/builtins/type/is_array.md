---
title: "is_array()"
description: "Lowers `is_array()`: true for statically-known arrays/hashes, or a boxed Mixed/Union value"
sidebar:
  order: 400
---

## is_array()

```php
function is_array(mixed $value): bool
```

Lowers `is_array()`: true for statically-known arrays/hashes, or a boxed Mixed/Union value

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_array.md).

