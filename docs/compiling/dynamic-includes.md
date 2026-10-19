---
title: "Dynamic includes"
description: "Closing include/require sites whose path is only known at run time with an elephc.includes.toml manifest, and the --dynamic-includes and --include-manifest flags."
sidebar:
  order: 10
---

elephc compiles a closed graph of source files. An `include` or `require` whose path
the resolver can fold at compile time (literals, `.` concatenation, `const`/`define()`
constants, `__DIR__`, and `PHP_VERSION` conditions, see below) is compiled like any
other file. A path that cannot be folded (a variable, a call result, a property) is a
compile error unless a per-application manifest, `elephc.includes.toml`, closes that
site: it lists the files the site may include, or refuses the site explicitly.

Files listed as candidates are compiled into the binary; nothing is read from disk as
PHP source at run time. Includes that run inside `eval()` keep using the runtime script
cache described in [OPcache](../php/opcache.md#the-runtime-script-cache).

## Finding the sites

```bash
elephc --dynamic-includes=report app.php
```

Report mode walks the whole reachable program, autoloaded files included, and prints
every non-foldable site instead of stopping at the first one:

```text
dynamic include sites: 2 (non-foldable include/require paths)
  1. src/Loader.php@App\Loader::load#1
     at /path/to/app/src/Loader.php:14
     expr: $this->dir . '/' . $name . '.php'
     md5: 6c9176ee051e698ff972c21e295a5af9
     status: unlisted (no entry in /path/to/app/elephc.includes.toml)
  ...
```

Each site's status is `unlisted`, `candidates [...]`, `deny: <reason>` or
`STALE md5 (manifest pins ...)`. The report ends with a TOML stub for every unlisted
site, lists manifest entries that match no site, and names any autoloaded file that
failed to compile for another reason (its sites may be missing from the list).
Report mode exits successfully and writes no binary.

## Site identity

A site id is `<file relative to the application root>@<scope>#<ordinal>`:

- the scope is `Class::method` (closures inside a method count as the method),
  `ns\function` for a function, or `{main}` for top-level code;
- the ordinal counts, in source order and starting at 1, the non-literal include paths
  of that scope;
- the application root is the directory that holds the manifest.

For example: `vendor/acme/config/src/Repository.php@Acme\Config\Repository::load#1`.

## Manifest location

- The manifest passed with `--include-manifest <path>`, or else
- the first `elephc.includes.toml` found walking up from the directory of the file that
  holds the site.

## Format

```toml
version = 1

[[site]]
id = 'src/Loader.php@App\Loader::load#1'
md5 = '6c9176ee051e698ff972c21e295a5af9'    # md5 of the file that CONTAINS the site
candidates = ['config/*.php', 'lang/{en,es}/*.php']

[[site]]
id = 'src/Cache.php@App\Cache::restore#1'
md5 = '0b8f1c7e2d3a4b5c6d7e8f9a0b1c2d3e'
deny = "compiled caches are not supported"
```

- `version` is optional; when present it must be `1`. Unknown keys are rejected.
- Each `[[site]]` declares exactly one of `candidates` (a non-empty list of globs) or
  `deny` (a reason string). A site listed twice is an error.
- Globs are relative to the application root and support `*`, `?`, `[...]` (with `!`
  negation), `{a,b}` alternatives and `**`. A glob that matches no file is a compile
  error.
- `md5` pins the file that contains the site. If the file changed (a dependency update,
  a local patch), the md5 no longer matches and the site is a compile error until it is
  reviewed and the md5 updated. Report mode shows it as `STALE md5`.

## Semantics

**`candidates`.** The path expression is evaluated once, then resolved like
php_resolve_path: an absolute path, or one starting with `./` or `../`, is resolved only
against the current working directory; a bare relative path (`conf/a.php`) is looked up
in the current working directory first (the include path is fixed to `.:`) and then next
to the file that holds the site. The `realpath()` of the result is compared with each
candidate's canonical path, and the matching branch is a static include of that
candidate. Return values (also nested), `_once` bookkeeping (shared with static
includes of the same file), the variable scope (`$this`, the caller's locals, names an
`extract()` created) and declarations behave as for a static include.

**Missing file.** `include` emits PHP's two `E_WARNING`s (`include(x): Failed to open
stream: No such file or directory` and `include(): Failed opening 'x' for inclusion
(include_path='.:')`), which go through `set_error_handler`, `error_reporting` and `@`,
and evaluates to `false`. `require` throws `Error: Failed opening required '...'`.

**Existing file outside the candidates.** Throws `\Error` naming the resolved path, the
site id and the number of candidates.

**`deny`.** The site throws `\Error` with the site id and the reason. In value position
the include is replaced by a `throw` expression before the path is evaluated.

**Version folding.** In a `require COND ? A : B` path, a condition that only reads
`PHP_VERSION`, `PHP_VERSION_ID`, `PHP_MAJOR_VERSION`, `PHP_MINOR_VERSION`,
`PHP_RELEASE_VERSION` or `version_compare(PHP_VERSION, 'x'[, op])` is folded with the
values of the selected `--php-version` profile, using PHP 8's comparison rules (`"8.5.0"
< 8.2` is a string comparison). An unqualified name is not folded when the site's
namespace declares its own `version_compare` or version constant in the same file, or
aliases one with `use function` / `use const`; the site then stays dynamic.

## Manifest entries that match no site

- If the entry's file is part of the program, the entry is a compile error (a stale
  manifest or a misspelled id).
- If the file is not part of the program (another entry point that shares the manifest,
  or code that is not reached), it is a warning.

## Limitations

- Every candidate is inlined at its site, so a candidate must compile in the scope of
  that site: a variable the candidate reads and the site never defines is a checker
  error.
- The include path is fixed to `.:`.
- A declaration of `version_compare` or a version constant in another file of the same
  namespace is not seen by the version folding.
- In value position, a `deny` site does not evaluate the path expression before
  throwing; in statement position it does. Only the side effects of that expression
  differ.

## Flags

| Flag | Values | Default | Description |
|---|---|---|---|
| `--dynamic-includes=MODE` | `strict`, `report` | `strict` | `strict` refuses non-foldable sites without a manifest entry. `report` lists every site and stops. |
| `--include-manifest <path>` / `--include-manifest=<path>` | path to a file | search upward | Use this manifest for every site instead of searching upward from each site's file. A relative path is resolved against the current directory. |

See also the [CLI reference](cli-reference.md#input-and-output).
