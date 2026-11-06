---
title: "pcntl_daemon()"
description: "Detaches the surviving child into a background daemon process."
---

## pcntl_daemon()

```php
function pcntl_daemon(bool $no_chdir = false, bool $no_close = false): bool
```

Detaches the surviving child into a background daemon process.

**Parameters**:
- `$no_chdir` (`bool`), default `false`, optional
- `$no_close` (`bool`), default `false`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_daemon` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_daemon.md).
