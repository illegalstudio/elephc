---
title: "mysqli_warning_count()"
description: "Returns how many warnings the last query raised."
sidebar:
  order: 183
---

## mysqli_warning_count()

```php
function mysqli_warning_count(mixed $mysql): int
```

Returns how many warnings the last query raised.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected mysqli prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_warning_count` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_warning_count.md).
