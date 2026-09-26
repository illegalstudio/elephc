---
title: "mysqli_fetch_all()"
description: "Returns every remaining row of a result at once."
sidebar:
  order: 117
---

## mysqli_fetch_all()

```php
function mysqli_fetch_all(mixed $result, int $mode = 2): array
```

Returns every remaining row of a result at once.

**Parameters**:
- `$result` (`mixed`)
- `$mode` (`int`), default `2`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_all` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_all.md).
