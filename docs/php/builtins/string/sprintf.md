---
title: "sprintf()"
description: "Returns a formatted string."
sidebar:
  order: 384
---

## sprintf()

```php
function sprintf(string $format, ...$values): string
```

Returns a formatted string.

**Parameters**:
- `$format` (`string`)
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/formatting/sprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/formatting/sprintf.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sprintf.md).

