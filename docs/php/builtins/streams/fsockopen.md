---
title: "fsockopen()"
description: "Open Internet or Unix domain socket connection."
sidebar:
  order: 350
---

## fsockopen()

```php
function fsockopen(string $hostname, int $port, int $error_code = null, string $error_message = null, float $timeout = null): mixed
```

Open Internet or Unix domain socket connection.

**Parameters**:
- `$hostname` (`string`)
- `$port` (`int`)
- `$error_code` (`int`), passed by reference, default `null`, optional
- `$error_message` (`string`), passed by reference, default `null`, optional
- `$timeout` (`float`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fsockopen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fsockopen.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fsockopen` is implemented in the compiler, see [the internals page](../../../internals/builtins/streams/fsockopen.md).

