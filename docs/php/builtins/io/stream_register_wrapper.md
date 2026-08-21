---
title: "stream_register_wrapper()"
description: "Register a URL wrapper implemented as a PHP class (alias of stream_wrapper_register)."
sidebar:
  order: 246
---

## stream_register_wrapper()

```php
function stream_register_wrapper(string $protocol, string $class, int $flags = 0): bool
```

Register a URL wrapper implemented as a PHP class (alias of stream_wrapper_register).

**Parameters**:
- `$protocol` (`string`)
- `$class` (`string`)
- `$flags` (`int`), default `0`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_register_wrapper.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_register_wrapper.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_register_wrapper` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_register_wrapper.md).
