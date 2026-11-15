---
title: "elephc_dump()"
description: "In a --debug build, records a bounded, formatted value snapshot in a capture or writes it to stderr; an invalid destination raises ValueError."
---

## elephc_dump()

```php
function elephc_dump(mixed $value, ?string $label = null, string $to = 'auto'): void
```

In a --debug build, records a bounded, formatted value snapshot in a capture or writes it to stderr; an invalid destination raises ValueError.

**Parameters**:
- `$value` (`mixed`)
- `$label` (`?string`), default `null`, optional
- `$to` (`string`), default `'auto'`, optional

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `elephc_dump` is implemented in the compiler, see [the internals page](../../../internals/builtins/debug/elephc_dump.md).
