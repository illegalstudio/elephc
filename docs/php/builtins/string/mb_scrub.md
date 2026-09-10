---
title: "mb_scrub()"
description: "Replaces malformed encoded units using the current substitution setting."
sidebar:
  order: 837
---

## mb_scrub()

```php
function mb_scrub(string $string, ?string $encoding = null): string
```

Replaces malformed encoded units using the current substitution setting.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_scrub.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_scrub.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_scrub` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_scrub.md).
