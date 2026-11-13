#!/usr/bin/env bash
# Fail image publication when its advertised execution contract is incomplete.
set -euo pipefail
role=${1:?image role required}
rustc --version
cargo nextest --version
gdb --version
for tool in cc ar git curl python3 m4 pkg-config; do command -v "$tool"; done
case "$role" in
  smoke|smoke22)
    ! pkg-config --exists libpcre2-8
    test ! -f /usr/include/pcre2.h
    ;;
  ci|pdo|php-nts|php-ts|reference84|reference85)
    pkg-config --exists libpcre2-8 libpq openssl odbc
    valgrind --version
    command -v kinit
    ;;
  kdc)
    command -v krb5kdc
    command -v kdb5_util
    ;;
  *) exit 1 ;;
esac
if [[ "$role" == php-* ]]; then
  expected=0
  [[ "$role" != php-ts ]] || expected=1
  php -n -r 'exit(PHP_VERSION_ID < 80500 || PHP_VERSION_ID >= 80600 || PHP_ZTS != (int)$argv[1] || PHP_DEBUG);' "$expected"
  /opt/elephc/php-debug/bin/php -n -r 'exit(PHP_ZTS != (int)$argv[1] || !PHP_DEBUG);' "$expected"
  test -f /opt/elephc/php-debug/lib/libphp.so
  php-config --includes
fi
if [[ "$role" == pdo ]]; then
  odbcinst -q -d -n 'ODBC Driver 18 for SQL Server'
  python3 -c 'import ctypes; ctypes.CDLL("/opt/elephc/oracle-client/libclntsh.so")'
fi

if [[ "$role" == reference* ]]; then
  minor=80500
  [[ "$role" != reference84 ]] || minor=80400
  php -n -r 'exit(intdiv(PHP_VERSION_ID,100) !== intdiv((int)$argv[1],100) || !extension_loaded("tokenizer") || !extension_loaded("dom") || !extension_loaded("mbstring"));' "$minor"
  composer --version
  /opt/elephc/phpunit/vendor/bin/phpunit --version
fi

if [[ "$role" == ci ]]; then
  python3 /opt/elephc/verify_libclang.py
fi
