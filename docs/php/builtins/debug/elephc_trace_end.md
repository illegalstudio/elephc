---
title: "elephc_trace_end()"
description: "In a --debug build, closes one named trace segment opened by elephc_trace_start()."
---

## elephc_trace_end()

```php
function elephc_trace_end(string $name): void
```

In a --debug build, closes one named trace segment opened by elephc_trace_start().

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

For how `elephc_trace_end` is implemented in the compiler, see [the internals page](../../../internals/builtins/debug/elephc_trace_end.md).
