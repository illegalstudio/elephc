---
title: "pcntl_signal_dispatch()"
description: "Invokes callbacks for every signal currently pending in PCNTL's queue."
---

## pcntl_signal_dispatch()

```php
function pcntl_signal_dispatch(): bool
```

Invokes callbacks for every signal currently pending in PCNTL's queue.

**Parameters**: none.

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_signal_dispatch` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_signal_dispatch.md).
