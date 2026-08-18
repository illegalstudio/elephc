---
title: "pcntl_setqos_class()"
description: "Changes the current macOS thread quality-of-service class."
sidebar:
  order: 340
---

## pcntl_setqos_class()

```php
function pcntl_setqos_class(mixed $qos_class = Pcntl\QosClass::Default): void
```

Changes the current macOS thread quality-of-service class.

**Parameters**:
- `$qos_class` (`mixed`), default `Pcntl\QosClass::Default`, optional

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_setqos_class.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_setqos_class.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_setqos_class` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_setqos_class.md).
