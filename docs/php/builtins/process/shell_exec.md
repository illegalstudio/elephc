---
title: "shell_exec()"
description: "Executes a command via the shell and returns the complete output as a string."
sidebar:
  order: 330
---

## shell_exec()

```php
function shell_exec(string $command): string
```

Executes a command via the shell and returns the complete output as a string.

**Parameters**:
- `$command` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/shell_exec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/shell_exec.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `shell_exec` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/shell_exec.md).
