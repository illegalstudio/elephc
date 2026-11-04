# Symfony Demo fixture

The official [Symfony Demo application](https://github.com/symfony/demo) is the largest
end-to-end target for elephc, next to `examples/symfony-app`. It exercises everything that app
does not: Doctrine ORM on SQLite, forms, the security firewall, the translator, intl formatting,
AssetMapper and a large Twig surface.

**No application code lives in this repository.** The Demo is Symfony's code, not elephc's, so
this directory holds only this README. Recreate the application here with the steps below.
Everything you create in this directory is git-ignored, except this file.

## 1. Install the Demo

Requirements:
- **PHP >= 8.4** (developed against 8.5.10), with `pdo_sqlite`, `intl`, `ctype` and `iconv`;
- [Composer](https://getcomposer.org/).

`composer create-project` needs an empty target, so create the application next to this
directory, then move it in:

```sh
# from the repository root
composer create-project symfony/symfony-demo /tmp/symfony-demo
cp -R /tmp/symfony-demo/. examples/symfony-demo/
git checkout -- examples/symfony-demo/README.md   # the copy overwrote this file with the Demo's own
cd examples/symfony-demo
composer install --no-dev
```

`--no-dev` is the production setup and it matters here: the dev packages register Composer
`files` (phpstan's phar bootstrap, react/promise) that a closed-world build would otherwise
compile.

The measurements in this repository were taken with the following versions:
- `symfony/framework-bundle`, `symfony/security-bundle`, `symfony/form`,
  `symfony/translation`, `symfony/asset-mapper`, `symfony/runtime`: 8.1.0;
- `doctrine/orm` 3.6.7, `doctrine/dbal` 4.4.3;
- `twig/twig` 3.27.1.

The SQLite fixture data ships with the Demo in `data/database.sqlite`.

## 2. Native packages

The Demo links native packages that elephc builds and caches itself. Write an `elephc.toml` at the
Demo root:

```toml
[native]
schema = 1

[native.dependencies]
pcre2 = "10.47"
oniguruma = "6.9.10"
curl = "8.21.0"
libssh2 = "1.11.1"
nghttp2 = "1.70.0"
openssl = "3.5.8"
zlib = "1.3.2"
libxml2 = "2.15.3"
gmp = "6.3.0"
libmagic = "5.46"
```

then resolve and install them once:

```sh
elephc native install --target macos-aarch64
```

## 3. Check it with php first

Build the production cache and the assets, then serve the Demo with php's built-in server:

```sh
APP_ENV=prod APP_DEBUG=0 php bin/console cache:clear
APP_ENV=prod APP_DEBUG=0 php bin/console asset-map:compile
APP_ENV=prod APP_DEBUG=0 php -n -d variables_order=EGPCS -S 127.0.0.1:8000 -t public public/index.php
```

`-n -d variables_order=EGPCS` matters: without it php's own `php.ini` may drop the environment
from `$_SERVER`, Symfony then reads `APP_ENV` from `.env` and serves the **dev** pages. That
server is the reference every elephc measurement on the Demo is compared against, byte for byte.
Sign in as `jane_admin` / `kitten` (admin) or `john_user` / `kitten`.

## 4. Compile it with elephc

Build from a fresh production cache, with Symfony's own container preload as the preload root,
the same way `opcache.preload` is configured for php in production:

```sh
cd examples/symfony-demo
APP_ENV=prod APP_DEBUG=0 php bin/console cache:clear
APP_ENV=prod APP_DEBUG=0 ../../target/release/elephc --web --timings \
  --php-version "$(php -n -r 'echo PHP_VERSION;')" \
  --ini opcache.enable_cli=1 \
  --ini opcache.preload=$PWD/var/cache/prod/App_KernelProdContainer.preload.php \
  --output-dir /tmp/symfony-demo-web public/index.php
APP_ENV=prod APP_DEBUG=0 /tmp/symfony-demo-web/index --listen 127.0.0.1:8001 --workers 2
```

Clear the cache before **every** build: a cache directory keeps the container dumps of other
debug modes, and the preload closure compiles all of them.

Class files elephc cannot compile yet are handed to the runtime autoloader and interpreted. Each
one is reported as a `warning: autoload: '…' is not compiled (…)` line, and that list is the
Demo's gap list. A backend refusal re-runs the compiler once with those files handed back
(`docs/compiling/compilation-pipeline.md`).

## What is never committed

- `vendor/**`, from `composer install`.
- `var/**`, generated and environment-specific.
- `elephc.toml` / `elephc.lock`, written from section 2.
- Compiled elephc output (`public/index*`, `*.s`, `*.o`). A single `.s` for the Demo is over a
  gigabyte; write builds outside the source tree with `--output-dir`.
