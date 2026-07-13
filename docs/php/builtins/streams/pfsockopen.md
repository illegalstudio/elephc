---
title: "pfsockopen()"
description: "Open persistent Internet or Unix domain socket connection."
sidebar:
  order: 338
---

## pfsockopen()

```php
function pfsockopen(string $hostname, int $port, int $error_code = null, string $error_message = null, float $timeout = null): mixed
```

Open persistent Internet or Unix domain socket connection.

**Parameters**:
- `$hostname` (`string`)
- `$port` (`int`)
- `$error_code` (`int`), passed by reference, default `null`, optional
- `$error_message` (`string`), passed by reference, default `null`, optional
- `$timeout` (`float`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/pfsockopen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/pfsockopen.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `pfsockopen` is implemented in the compiler, see [the internals page](../../../internals/builtins/streams/pfsockopen.md).

