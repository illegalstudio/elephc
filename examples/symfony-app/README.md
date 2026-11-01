# Symfony fixture

A real Symfony application is the large end-to-end target for elephc. The `--web` and
`bin/console` compilations are measured against it and verified on it.

**No application code lives in this repository.** The application is Symfony's code, not
elephc's, so this directory holds only this README. Recreate the app here with the steps
below. Everything you create in this directory is git-ignored, except this file.

## 1. Install Symfony

Requirements:
- **PHP >= 8.4** (developed against 8.5.10), with `ext-ctype` and `ext-iconv`;
- [Composer](https://getcomposer.org/).

`composer create-project` needs an empty target, so create the app next to this directory, then
move it in:

```sh
# from the repository root
composer create-project symfony/skeleton:"8.1.*" /tmp/symfony-app
cp -R /tmp/symfony-app/. examples/symfony-app/
cd examples/symfony-app
composer require "symfony/twig-bundle:8.1.*"
```

The measurements in this repository were taken with the following versions:
- `symfony/framework-bundle` 8.1.4;
- `symfony/console` 8.1.4;
- `symfony/runtime` 8.1.0;
- `symfony/twig-bundle` 8.1.7;
- `symfony/yaml` 8.1.2;
- `twig/twig` 3.28.0.

## 2. Add the fixture's controller and templates

Create one controller, `src/Controller/HelloController.php`. It is a `final` class extending
`AbstractController`, with one action per route, each declared through a `#[Route]` attribute.
The skeleton's `config/routes.yaml` already imports attribute routes. Each route is chosen to
exercise one part of Symfony that the 404 path never reaches.

| Route | Methods | Response |
|---|---|---|
| `/` | GET | renders `hello.html.twig` with `greeting` = `Hello from Symfony`, `engine` = `Twig`, `words` = `['compiled', 'ahead', 'of', 'time']` |
| `/plain` | GET | a plain `Response`: `hello from a compiled controller` followed by a newline |
| `/greet/{name}` | GET | a plain `Response`: `hello `, then the upper-cased `{name}`, then a newline (route argument resolution) |
| `/echo` | GET, POST | a `JsonResponse` with `method`, `query` (all query parameters), `post` (all request parameters) and `path` (the path info) |

The `/` route needs two templates in `templates/`:
- `base.html.twig`: the layout the skeleton's Twig recipe creates.
- `hello.html.twig`: extends `base.html.twig`. Its title block is `Hello`. Its body block has an
  `<h1>` showing `greeting`, a paragraph `Rendered by <engine>.`, and a list with one item per
  word, rendered as `<loop.index>. <word upper-cased>`.

Twig compiles each template to a PHP class at run time and `require`s it. Rendering `/` therefore
also exercises the interpreted-include path.

The default environment is `dev`, and in `dev` the routes are not wired: `php -S` itself answers
404 on every one of them. Pass `APP_ENV=prod APP_DEBUG=0` to exercise the routes.

## 3. Check it with php

Build the production cache, then serve the app with php's built-in server:

```sh
APP_ENV=prod APP_DEBUG=0 php bin/console cache:clear
APP_ENV=prod APP_DEBUG=0 php -d variables_order=EGPCS -S 127.0.0.1:8000 -t public public/index.php
curl -s http://127.0.0.1:8000/plain     # -> "hello from a compiled controller"
```

`php -S` on this app is the reference that every elephc measurement is compared against: byte for
byte for responses, and interleaved for latency. `/no-such-route` exercises the 404 path, through
Symfony's `ErrorHandler`.

## 4. Compile it with elephc

### The preload file

elephc compiles a closed world: only the code reachable from the entry point, plus what
`opcache.preload` names. Write a preload file, `elephc-preload.php`, at the app root. It lists:

1. `require` of `var/cache/prod/App_KernelProdContainer.preload.php`, Symfony's own preload of the
   DI container.
2. `require_once` of every compiled Twig template under `var/cache/prod/twig/`. Write each one as
   a literal path: `require` targets are resolved statically, so a loop or a glob preloads nothing.
3. `opcache_compile_file()` of `var/cache/prod/App_KernelProdContainer.bundles.php` and
   `var/cache/prod/url_matching_routes.php`. These are value-returning cache files that a request
   reaches through computed paths.

The template file names are content hashes, so regenerate the preload file after every
`cache:clear` or `cache:warmup` that changes a template. Stale entries are skipped silently.

### Build

```sh
# from the repository root
./target/release/elephc --web --timings \
  --ini opcache.preload=$PWD/examples/symfony-app/elephc-preload.php \
  examples/symfony-app/public/index.php
```

**`--ini opcache.preload=FILE` is ignored unless FILE is reachable from the entry point's
statically resolved include graph.** The compiler warns once and continues, and the warning is
easy to miss in a long build log. When it is ignored, the DI container is not compiled and the
interpreter covers it silently. The binary still answers every route byte-identically, but about
2.3 times slower. Check the binary's size, and check that it carries the container, before
trusting a measurement.

For `bin/console`, give the build both preloads, the `dev` one first and the `prod` one last, and
clear both caches before every build. A stale container dump from another debug mode is compiled
too.

## What is never committed

- `vendor/**`, from `composer install`.
- `var/cache/**`, generated and environment-specific.
- Compiled elephc output (`public/index*`, `*.s`, `*.o`). A single `.s` for this app is over a
  gigabyte.
- The application's own files: the controller, the templates and the configuration.
