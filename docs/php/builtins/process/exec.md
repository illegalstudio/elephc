---
title: "exec()"
description: "Executes an external program and returns the last line of output."
sidebar:
  order: 309
---

## exec()

```php
function exec(string $command): string
```

Executes an external program and returns the last line of output.

**Parameters**:
- `$command` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/exec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/exec.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `exec` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/exec.md).

