---
title: "stream_context_get_options()"
description: "Retrieves options for the specified stream context."
sidebar:
  order: 197
---

## stream_context_get_options()

```php
function stream_context_get_options(resource $context): array
```

Retrieves options for the specified stream context.

**Parameters**:
- `$context` (`resource`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_get_options.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_get_options.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_context_get_options` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_context_get_options.md).

