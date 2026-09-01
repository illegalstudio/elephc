---
title: "popen()"
description: "Opens process file pointer."
sidebar:
  order: 366
---

## popen()

```php
function popen(string $command, string $mode): mixed
```

Opens process file pointer.

**Parameters**:
- `$command` (`string`)
- `$mode` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/popen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/popen.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `popen` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/popen.md).
