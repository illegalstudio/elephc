---
title: "LFC Source Files"
description: "Tagless .lfc source files, mixed PHP/LFC projects, and per-file strict-mode behavior."
sidebar:
  order: 1
---

An `.lfc` file is elephc source from its first byte to its last byte. It has no
PHP opening or closing tags:

```text
ifdef DEBUG {
    echo "debug\n";
}

$pointer = ptr_null();
echo ptr_is_null($pointer) ? "null\n" : "set\n";
```

```bash
elephc build --define DEBUG main.lfc
./main
```

The normal output naming rules apply: `main.lfc` produces `main`, while
`--emit-asm` and `--source-map` produce `main.s` and `main.map`.

## Source contract

In LFC mode:

- the complete file is parsed as code;
- `<?php` and `?>` are invalid outside strings and comments;
- plain text is a syntax error, not implicit output;
- output comes only from explicit constructs such as `echo` and `print`;
- an optional UTF-8 BOM is ignored without shifting source coordinates;
- every elephc language extension and extension builtin is available.

Tag-shaped text remains ordinary data when it occurs inside a string, heredoc,
nowdoc, or comment. Only tags encountered at a code boundary are rejected.

Only a case-insensitive `.lfc` suffix selects tagless mode. `.php`, `.inc`,
extensionless files, and all other paths retain the existing tagged-PHP
contract and must begin with `<?php`.

## Mixed PHP and LFC projects

Each physical file selects its own mode. The entry point does not impose its
mode on the files it loads:

```php
<?php
// main.php
require __DIR__ . '/native.lfc';
```

```text
// native.lfc
echo ptr_is_null(ptr_null()) ? "LFC\n" : "unexpected\n";
```

The same rule applies in the other direction: an LFC entry can include tagged
PHP. `include`, `require`, their `_once` variants, Composer `autoload.files`,
PSR-4, PSR-0, classmap, and supported SPL autoload rules all classify the
physical target before parsing it. Magic constants such as `__FILE__` and
`__DIR__` retain the real `.lfc` path.

Composer directory scans consider both `.php` and `.lfc` files. Explicit paths
still work with any tagged-PHP suffix as before. The default observable value
of `spl_autoload_extensions()` remains `.inc,.php`; LFC support does not alter
PHP's runtime-facing default.

## `--strict-php`

`--strict-php` is requested for the compilation but enforced per physical
PHP-mode user file:

- tagged PHP rejects elephc syntax and hides extension builtins;
- LFC always keeps the full elephc surface;
- compiler-generated preludes remain extension-capable;
- PHP included or autoloaded from an LFC entry is still audited;
- LFC included or autoloaded from a strict PHP entry is not restricted.

The call site's file controls runtime-selected behavior too. Direct and dynamic
calls, first-class callables, `function_exists()`, `is_callable()`, callback
dispatch, and `eval()` all observe that call site's PHP or LFC builtin profile.
The compiler therefore supports strict PHP and LFC call sites in the same
binary without leaking one profile into the other.

`--strict-php` can be combined with `--define`. An LFC `ifdef` consumes the
defined symbol normally. A PHP-mode `ifdef` is rejected by the strict audit
before its branch can be removed, including when the symbol is not defined.

## PHP compatibility

Choose `.php` when the file should remain valid tagged PHP and optionally pass
the strict compatibility audit. Choose `.lfc` when the file is specifically
elephc source and benefits from tagless syntax or compiler extensions.

The two formats share the same parser, type checker, optimizer, EIR, code
generator, runtime, supported targets, diagnostics, and output formats. LFC is
a source-file profile, not a second language backend.
