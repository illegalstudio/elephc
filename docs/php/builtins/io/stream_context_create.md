---
title: "stream_context_create()"
description: "Creates a stream context."
sidebar:
  order: 208
---

## stream_context_create()

```php
function stream_context_create(array $options = null, array $params = null): mixed
```

Creates a stream context.

**Parameters**:
- `$options` (`array`), default `null`, optional
- `$params` (`array`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_create.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_create.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_context_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_context_create.md).

