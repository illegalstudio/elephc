---
title: "stream_context_set_params()"
description: "Sets parameters on the specified context."
sidebar:
  order: 216
---

## stream_context_set_params()

```php
function stream_context_set_params(resource $context, array $params): bool
```

Sets parameters on the specified context.

**Parameters**:
- `$context` (`resource`)
- `$params` (`array`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_set_params.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_set_params.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_context_set_params` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_context_set_params.md).
