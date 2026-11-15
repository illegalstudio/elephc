---
title: "elephc_trace_start()"
description: "In a --debug build, opens a named, nestable trace segment in an active capture."
---

## elephc_trace_start()

```php
function elephc_trace_start(string $name): void
```

In a --debug build, opens a named, nestable trace segment in an active capture.

**Parameters**:
- `$name` (`string`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `elephc_trace_start` is implemented in the compiler, see [the internals page](../../../internals/builtins/debug/elephc_trace_start.md).
