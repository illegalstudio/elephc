---
title: "mysqli_connect()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 104
---

## mysqli_connect()

```php
function mysqli_connect(string $hostname = null, string $username = null, string $password = null, string $database = null, int $port = null, string $socket = null): mixed
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$hostname` (`string`), default `null`, optional
- `$username` (`string`), default `null`, optional
- `$password` (`string`), default `null`, optional
- `$database` (`string`), default `null`, optional
- `$port` (`int`), default `null`, optional
- `$socket` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_connect` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_connect.md).
