---
title: "mysqli_warning_count()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 181
---

## mysqli_warning_count()

```php
function mysqli_warning_count(mixed $mysql): int
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_warning_count` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_warning_count.md).
