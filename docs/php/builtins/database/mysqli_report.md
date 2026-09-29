---
title: "mysqli_report()"
description: "Selects which mysqli conditions raise exceptions or warnings."
sidebar:
  order: 154
---

## mysqli_report()

```php
function mysqli_report(int $flags): bool
```

Selects which mysqli conditions raise exceptions or warnings.

**Parameters**:
- `$flags` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_report` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_report.md).
