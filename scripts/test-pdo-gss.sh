#!/usr/bin/env bash
# Runs the focused PDO PostgreSQL GSSAPI integration tests against an ephemeral
# MIT Kerberos realm and PostgreSQL 16 server.
#
# Requirements: Docker, cargo, kinit, and a libpq development package. The
# caller's Cargo target/cache are reused; every Docker resource is removed on exit.

set -euo pipefail

REALM="ELEPHC.TEST"
CLIENT_PRINCIPAL="elephc_gss@$REALM"
RUN_ID="$$"
NETWORK="elephc-pdo-gss-$RUN_ID"
KDC_CONTAINER="elephc-pdo-kdc-$RUN_ID"
PG_CONTAINER="elephc-pdo-pg-gss-$RUN_ID"
KDC_PORT="${ELEPHC_GSS_KDC_PORT:-10088}"
PG_PORT="${ELEPHC_GSS_PG_PORT:-55432}"
FIXTURE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/elephc-pdo-gss.XXXXXX")"

cleanup() {
    docker rm -f "$PG_CONTAINER" "$KDC_CONTAINER" >/dev/null 2>&1 || true
    docker network rm "$NETWORK" >/dev/null 2>&1 || true
    rm -rf "$FIXTURE_DIR"
}

trap cleanup EXIT INT TERM

for command in docker cargo kinit; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "PDO GSSAPI test requires '$command'" >&2
        exit 1
    fi
done

cat >"$FIXTURE_DIR/krb5-kdc.conf" <<EOF
[libdefaults]
    default_realm = $REALM
    dns_lookup_kdc = false
    dns_lookup_realm = false
    dns_canonicalize_hostname = false
    rdns = false
    forwardable = true

[realms]
    $REALM = {
        kdc = $KDC_CONTAINER
        admin_server = $KDC_CONTAINER
    }

[domain_realm]
    .elephc.test = $REALM
    elephc.test = $REALM
EOF

cat >"$FIXTURE_DIR/krb5-client.conf" <<EOF
[libdefaults]
    default_realm = $REALM
    dns_lookup_kdc = false
    dns_lookup_realm = false
    dns_canonicalize_hostname = false
    rdns = false
    forwardable = true

[realms]
    $REALM = {
        kdc = 127.0.0.1:$KDC_PORT
        admin_server = 127.0.0.1:$KDC_PORT
    }

[domain_realm]
    .elephc.test = $REALM
    elephc.test = $REALM
EOF

cat >"$FIXTURE_DIR/kdc.conf" <<EOF
[kdcdefaults]
    kdc_ports = 88
    kdc_tcp_ports = 88

[realms]
    $REALM = {
        database_name = /var/lib/krb5kdc/principal
        key_stash_file = /etc/krb5kdc/stash
        acl_file = /etc/krb5kdc/kadm5.acl
        max_life = 10h 0m 0s
        max_renewable_life = 7d 0h 0m 0s
        default_principal_flags = +preauth
    }
EOF

printf '*/admin@%s *\n' "$REALM" >"$FIXTURE_DIR/kadm5.acl"

docker network create "$NETWORK" >/dev/null

docker run -d \
    --name "$KDC_CONTAINER" \
    --hostname "$KDC_CONTAINER" \
    --network "$NETWORK" \
    -p "127.0.0.1:$KDC_PORT:88/tcp" \
    -p "127.0.0.1:$KDC_PORT:88/udp" \
    -v "$FIXTURE_DIR:/fixture" \
    debian:bookworm-slim \
    sh -ec '
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        apt-get install -y -qq krb5-kdc krb5-admin-server >/dev/null
        install -m 0644 /fixture/krb5-kdc.conf /etc/krb5.conf
        install -m 0644 /fixture/kdc.conf /etc/krb5kdc/kdc.conf
        install -m 0600 /fixture/kadm5.acl /etc/krb5kdc/kadm5.acl
        kdb5_util create -s -P elephc-master-password
        kadmin.local -q "addprinc -randkey postgres/pgsql.elephc.test@ELEPHC.TEST"
        kadmin.local -q "addprinc -randkey elephc_gss@ELEPHC.TEST"
        kadmin.local -q "ktadd -k /fixture/postgres.keytab postgres/pgsql.elephc.test@ELEPHC.TEST"
        kadmin.local -q "ktadd -k /fixture/client.keytab elephc_gss@ELEPHC.TEST"
        chmod 0644 /fixture/postgres.keytab /fixture/client.keytab
        touch /fixture/kdc-ready
        exec krb5kdc -n
    ' >/dev/null

for attempt in $(seq 1 90); do
    if [ -f "$FIXTURE_DIR/kdc-ready" ]; then
        break
    fi
    if ! docker inspect -f '{{.State.Running}}' "$KDC_CONTAINER" 2>/dev/null | grep -q true; then
        docker logs "$KDC_CONTAINER" >&2
        exit 1
    fi
    if [ "$attempt" -eq 90 ]; then
        docker logs "$KDC_CONTAINER" >&2
        echo "Kerberos KDC did not become ready" >&2
        exit 1
    fi
    sleep 2
done

docker run -d \
    --name "$PG_CONTAINER" \
    --hostname pgsql.elephc.test \
    --network "$NETWORK" \
    -p "127.0.0.1:$PG_PORT:5432" \
    -e POSTGRES_PASSWORD=test \
    -e POSTGRES_DB=testdb \
    postgres:16 >/dev/null

for attempt in $(seq 1 60); do
    if docker exec "$PG_CONTAINER" pg_isready -U postgres -d testdb >/dev/null 2>&1; then
        break
    fi
    if [ "$attempt" -eq 60 ]; then
        docker logs "$PG_CONTAINER" >&2
        echo "PostgreSQL GSSAPI fixture did not become ready" >&2
        exit 1
    fi
    sleep 2
done

if ! docker exec "$PG_CONTAINER" pg_config --configure | grep -q -- '--with-gssapi'; then
    echo "PostgreSQL fixture was built without GSSAPI support" >&2
    exit 1
fi

docker cp "$FIXTURE_DIR/krb5-kdc.conf" "$PG_CONTAINER:/tmp/krb5.conf"
docker cp "$FIXTURE_DIR/postgres.keytab" "$PG_CONTAINER:/tmp/postgres.keytab"
docker exec -u root "$PG_CONTAINER" sh -ec '
    install -o postgres -g postgres -m 0644 /tmp/krb5.conf /etc/krb5.conf
    install -o postgres -g postgres -m 0600 /tmp/postgres.keytab /var/lib/postgresql/postgres.keytab
    sed -i "1ihostgssenc testdb elephc_gss 0.0.0.0/0 gss include_realm=0 krb_realm=ELEPHC.TEST" /var/lib/postgresql/data/pg_hba.conf
    printf "\nkrb_server_keyfile = '\''/var/lib/postgresql/postgres.keytab'\''\n" >> /var/lib/postgresql/data/postgresql.conf
'
docker exec "$PG_CONTAINER" \
    psql -U postgres -d testdb -v ON_ERROR_STOP=1 \
    -c 'CREATE ROLE elephc_gss LOGIN'
docker restart "$PG_CONTAINER" >/dev/null

for attempt in $(seq 1 60); do
    if docker exec "$PG_CONTAINER" pg_isready -U postgres -d testdb >/dev/null 2>&1; then
        break
    fi
    if [ "$attempt" -eq 60 ]; then
        docker logs "$PG_CONTAINER" >&2
        echo "PostgreSQL GSSAPI fixture did not restart" >&2
        exit 1
    fi
    sleep 2
done

export KRB5_CONFIG="$FIXTURE_DIR/krb5-client.conf"
export KRB5CCNAME="FILE:$FIXTURE_DIR/client.ccache"
kinit -V -k -t "$FIXTURE_DIR/client.keytab" "$CLIENT_PRINCIPAL"

export ELEPHC_PDO_LIBPQ=1
export ELEPHC_PDO_GSS_REQUIRED=1
export ELEPHC_PG_GSS_DSN="pgsql:host=pgsql.elephc.test;hostaddr=127.0.0.1;port=$PG_PORT;dbname=testdb;user=elephc_gss;gssencmode=require;require_auth=gss;krbsrvname=postgres"
export ELEPHC_PG_GSS_EMPTY_CACHE="FILE:$FIXTURE_DIR/missing.ccache"

cargo build -p elephc-pdo --features libpq-gss
cargo test --features pdo-libpq-gss --test codegen_tests pgsql_gss -- --ignored --test-threads=1
