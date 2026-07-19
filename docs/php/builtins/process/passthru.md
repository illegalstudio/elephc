---
title: "passthru()"
description: "Executes an external program and passes its output directly."
sidebar:
  order: 311
---

## passthru()

```php
function passthru(string $command): void
```

Executes an external program and passes its output directly.

**Parameters**:
- `$command` (`string`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/passthru.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/passthru.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `passthru` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/passthru.md).

