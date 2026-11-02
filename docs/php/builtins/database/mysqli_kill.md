---
title: "mysqli_kill()"
description: "Asks the server to kill a MySQL thread."
sidebar:
  order: 149
---

## mysqli_kill()

```php
function mysqli_kill(mixed $mysql, int $process_id): bool
```

Asks the server to kill a MySQL thread.

**Parameters**:
- `$mysql` (`mixed`)
- `$process_id` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_kill` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_kill.md).
