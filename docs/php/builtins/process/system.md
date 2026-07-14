---
title: "system()"
description: "Executes an external program and displays the output."
sidebar:
  order: 317
---

## system()

```php
function system(string $command): string
```

Executes an external program and displays the output.

**Parameters**:
- `$command` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/system.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/system.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `system` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/system.md).

