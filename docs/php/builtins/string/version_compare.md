---
title: "version_compare()"
description: "Compares two \"PHP-standardized\" version number strings."
sidebar:
  order: 949
---

## version_compare()

```php
function version_compare(string $version1, string $version2, mixed $operator = null): mixed
```

Compares two "PHP-standardized" version number strings.

**Parameters**:
- `$version1` (`string`)
- `$version2` (`string`)
- `$operator` (`mixed`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/version_compare.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/version_compare.rs)).

**Examples**:

```php
// Full example: examples/versions-and-tokens/main.php
echo version_compare("1.10.0", "1.9.9");         // 1
var_dump(version_compare("8.1.4", "8.2.0", ">="));  // bool(false)
```

## Internals

For how `version_compare` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/version_compare.md).
