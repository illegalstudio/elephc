---
title: "is_finite()"
description: "Lowers `is_finite()` by rejecting NaN and both infinities."
sidebar:
  order: 234
---

## is_finite()

```php
function is_finite(float $num): bool
```

Lowers `is_finite()` by rejecting NaN and both infinities.

**Parameters**:
- `$num` (`float`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_finite` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/is_finite.md).

