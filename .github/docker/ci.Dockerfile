# Native Linux CI environments. Build source packages once, then execute jobs
# without package-manager access. Keep the managed-native smoke free of PCRE2
# development files so a system fallback cannot conceal resolver regressions.
ARG UBUNTU_VERSION=24.04
FROM ubuntu:${UBUNTU_VERSION} AS toolchain
ARG RUST_VERSION=1.98
ARG NEXTEST_VERSION=0.9.140
ENV DEBIAN_FRONTEND=noninteractive
COPY --chmod=0755 scripts/ci/apt_install.sh /usr/local/bin/elephc-ci-apt-install
RUN elephc-ci-apt-install --no-install-recommends \
        binutils build-essential ca-certificates curl file gdb git libbz2-dev \
        m4 netbase pkg-config python3 tzdata zlib1g-dev zstd \
    && rm -rf /var/lib/apt/lists/*
# Outside HOME: GitHub overrides HOME for actions and job containers.
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "${RUST_VERSION}" \
    && rustc --version && cargo --version
RUN case "$(dpkg --print-architecture)" in \
        amd64) nextest_platform=linux ;; \
        arm64) nextest_platform=linux-arm ;; \
        *) exit 1 ;; \
    esac \
    && curl -LsSf "https://get.nexte.st/${NEXTEST_VERSION}/${nextest_platform}" \
        | tar -xzf - -C /usr/local/cargo/bin \
    && cargo nextest --version

FROM toolchain AS smoke
RUN ! pkg-config --exists libpcre2-8 && ! test -f /usr/include/pcre2.h \
    && gdb --version

FROM toolchain AS ci-base
RUN elephc-ci-apt-install --no-install-recommends \
        freetds-dev krb5-user libaio1t64 libc6-dbg libpcre2-dev libpq-dev \
        libssl-dev odbc-postgresql unixodbc-dev unzip valgrind \
    && gdb --version && valgrind --version
RUN rm -rf /var/lib/apt/lists/*

FROM ci-base AS php
ARG PHP_ZTS=0
COPY scripts/phpext/build_host.py /opt/elephc/build_host.py
RUN set -eu; flags=""; if [ "$PHP_ZTS" = 1 ]; then flags=--zts; fi; \
    python3 /opt/elephc/build_host.py --prefix /opt/elephc/php $flags; \
    python3 /opt/elephc/build_host.py --prefix /opt/elephc/php-debug $flags --debug --with-valgrind
ENV PATH=/opt/elephc/php/bin:/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
RUN php -n -v && php-config --vernum \
    && /opt/elephc/php-debug/bin/php -n -v

FROM ci-base AS pdo
# The optional vendor suite runs only on x86_64. Vendor checks and signed
# Microsoft repository verification happen here, never in individual test jobs.
COPY --chmod=0755 scripts/ci/install_pdo_clients.sh /opt/elephc/install_pdo_clients.sh
RUN /opt/elephc/install_pdo_clients.sh && rm -rf /var/lib/apt/lists/*
ENV LD_LIBRARY_PATH=/opt/elephc/oracle-client

FROM toolchain AS kdc
RUN elephc-ci-apt-install --no-install-recommends krb5-kdc krb5-admin-server \
    && rm -rf /var/lib/apt/lists/*

FROM ci-base AS reference
ARG PHP_VERSION=8.5.11
COPY scripts/phpext/build_host.py /opt/elephc/build_host.py
RUN elephc-ci-apt-install --no-install-recommends libonig-dev libxml2-dev \
    && rm -rf /var/lib/apt/lists/* \
    && python3 /opt/elephc/build_host.py --prefix /opt/elephc/php --reference --version "$PHP_VERSION"
COPY --from=docker.io/library/composer:2.8 /usr/bin/composer /usr/local/bin/composer
ENV PATH=/opt/elephc/php/bin:/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
COPY examples/testing/composer.json /opt/elephc/phpunit/composer.json
RUN COMPOSER_ALLOW_SUPERUSER=1 composer require --working-dir=/opt/elephc/phpunit \
        --dev --no-interaction --no-progress --prefer-dist phpunit/phpunit \
    && /opt/elephc/phpunit/vendor/bin/phpunit --version

# libpq-sys generates bindings during the PDO producer build. Hosted VMs
# supplied libclang implicitly; runtime consumers do not need this parser.
FROM ci-base AS ci
COPY scripts/ci/verify_libclang.py /opt/elephc/verify_libclang.py
RUN elephc-ci-apt-install --no-install-recommends libclang1-18 libclang-common-18-dev \
    && python3 /opt/elephc/verify_libclang.py \
    && rm -rf /var/lib/apt/lists/*

# The default target remains the shared compiler/shard image.
FROM ci AS default
