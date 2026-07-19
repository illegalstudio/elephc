---
title: "stream_context_get_params()"
description: "Retrieves parameters from the specified stream context."
sidebar:
  order: 198
---

## stream_context_get_params()

```php
function stream_context_get_params(resource $context): array
```

Retrieves parameters from the specified stream context.

**Parameters**:
- `$context` (`resource`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_get_params.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_get_params.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_context_get_params` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_context_get_params.md).

