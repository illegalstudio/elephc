---
title: "stream_get_transports()"
description: "Retrieves list of registered socket transports."
sidebar:
  order: 222
---

## stream_get_transports()

```php
function stream_get_transports(): array
```

Retrieves list of registered socket transports.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/stream_get_transports.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/stream_get_transports.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_get_transports` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_get_transports.md).

