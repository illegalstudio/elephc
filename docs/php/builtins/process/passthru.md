---
title: "passthru()"
description: "Executes an external program and passes its output directly."
sidebar:
  order: 364
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

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/passthru.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/passthru.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `passthru` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/passthru.md).
