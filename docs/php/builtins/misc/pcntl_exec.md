---
title: "pcntl_exec()"
description: "Replaces the current process image with a program and optional arguments and environment."
sidebar:
  order: 330
---

## pcntl_exec()

```php
function pcntl_exec(string $path, mixed $args = [], mixed $env_vars = []): bool
```

Replaces the current process image with a program and optional arguments and environment.

**Parameters**:
- `$path` (`string`)
- `$args` (`mixed`), default `[]`, optional
- `$env_vars` (`mixed`), default `[]`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_exec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_exec.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_exec` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_exec.md).
