---
title: "mysqli_fetch_all()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 114
---

## mysqli_fetch_all()

```php
function mysqli_fetch_all(mixed $result, int $mode = 2): mixed
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$result` (`mixed`)
- `$mode` (`int`), default `2`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_all` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_all.md).
