---
title: "mysqli_poll()"
description: "Polls connections for completed async queries."
sidebar:
  order: 157
---

## mysqli_poll()

```php
function mysqli_poll(?array $read, ?array $error, array $reject, int $seconds, int $microseconds = 0): mixed
```

Polls connections for completed async queries.

**Parameters**:
- `$read` (`?array`), passed by reference
- `$error` (`?array`), passed by reference
- `$reject` (`array`), passed by reference
- `$seconds` (`int`)
- `$microseconds` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_poll` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_poll.md).
