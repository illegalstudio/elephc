---
title: "pcntl_exec()"
description: "Replaces the current process image with a program and optional arguments and environment."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_exec` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_exec.md).
