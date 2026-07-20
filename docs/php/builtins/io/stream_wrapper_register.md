---
title: "stream_wrapper_register()"
description: "Registers a URL wrapper implemented as a PHP class."
sidebar:
  order: 243
---

## stream_wrapper_register()

```php
function stream_wrapper_register(string $protocol, string $class, int $flags = 0): bool
```

Registers a URL wrapper implemented as a PHP class.

**Parameters**:
- `$protocol` (`string`)
- `$class` (`string`)
- `$flags` (`int`), default `0`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_wrapper_register.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_wrapper_register.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_wrapper_register` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_wrapper_register.md).

