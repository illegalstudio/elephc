---
title: "mysqli_report()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 151
---

## mysqli_report()

```php
function mysqli_report(int $flags): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$flags` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_report` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_report.md).
