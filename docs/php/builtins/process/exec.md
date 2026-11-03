---
title: "exec()"
description: "Executes an external program and returns the last line of output."
sidebar:
  order: 863
---

## exec()

```php
function exec(string $command, array $output = null, int $result_code = null): string
```

Executes an external program and returns the last line of output.

**Parameters**:
- `$command` (`string`)
- `$output` (`array`), passed by reference, default `null`, optional
- `$result_code` (`int`), passed by reference, default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/exec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/exec.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `exec` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/exec.md).
