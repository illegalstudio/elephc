---
title: "stream_get_wrappers()"
description: "Retrieves list of registered streams."
sidebar:
  order: 223
---

## stream_get_wrappers()

```php
function stream_get_wrappers(): array
```

Retrieves list of registered streams.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/stream_get_wrappers.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/stream_get_wrappers.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_get_wrappers` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_get_wrappers.md).

