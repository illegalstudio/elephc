#!/usr/bin/env bash
# Install checksum-verified vendor clients in the optional PDO execution image.
set -euo pipefail
[[ $(dpkg --print-architecture) == amd64 ]]
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl --fail --location --retry 5 --retry-all-errors \
  --output "$work/instantclient.zip" \
  https://download.oracle.com/otn_software/linux/instantclient/2326200v2/instantclient-basiclite-linux.x64-23.26.2.0.0.zip
test "$(cksum "$work/instantclient.zip" | awk '{print $1 " " $2}')" = '149323083 75797020'
unzip -q "$work/instantclient.zip" -d "$work"
mkdir -p /opt/elephc/oracle-client
cp -a "$work/instantclient_23_26/." /opt/elephc/oracle-client/
test -e /opt/elephc/oracle-client/libclntsh.so
libaio_path=$(find /usr/lib -name libaio.so.1t64 -print -quit)
test -n "$libaio_path"
ln -s "$libaio_path" /opt/elephc/oracle-client/libaio.so.1
curl --fail --location --retry 5 --retry-all-errors \
  --output "$work/microsoft-prod.deb" \
  https://packages.microsoft.com/config/ubuntu/24.04/packages-microsoft-prod.deb
dpkg -i "$work/microsoft-prod.deb"
ACCEPT_EULA=Y elephc-ci-apt-install msodbcsql18
odbcinst -q -d -n 'ODBC Driver 18 for SQL Server'
