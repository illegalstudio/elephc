---
title: "version_compare()"
description: "Compares two PHP-standardized version number strings."
sidebar:
  order: 1080
---

## version_compare()

```php
function version_compare(string $version1, string $version2, string $operator = null): mixed
```

Compares two PHP-standardized version number strings.

**Parameters**:
- `$version1` (`string`)
- `$version2` (`string`)
- `$operator` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected version_compare prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/version_compare.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/version_compare.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `version_compare` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/version_compare.md).
