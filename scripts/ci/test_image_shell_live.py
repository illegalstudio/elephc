#!/usr/bin/env python3
"""Exercise the container boundary against one published image without compiling Rust."""

import json
import os
import pwd
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading

ROOT = Path(__file__).resolve().parents[2]


def main():
    """Prove localhost reachability, env-file writes, Git ownership and the Cargo guard."""
    with tempfile.TemporaryDirectory(prefix='.image-shell-live.', dir=ROOT) as directory:
        work = Path(directory)
        image = sys.argv[1]
        env = dict(os.environ, RUNNER_TEMP=str(work), GITHUB_WORKSPACE=str(ROOT),
                   GITHUB_ENV=str(work / 'github-env'), ELEPHC_CI_EXEC_IMAGE=image,
                   ELEPHC_CI_EXEC_ROLE='ci', ELEPHC_IMAGE_TEST_SECRET='first\nsecond', ELEPHC_IMAGE_TEST_USER=pwd.getpwuid(os.getuid()).pw_name)
        (work / 'elephc-image-home/.cargo').mkdir(parents=True)
        fixture = work / 'checkout'
        subprocess.run(['git', 'init', '-q', str(fixture)], check=True)
        subprocess.run(['git', '-C', str(fixture), '-c', 'user.name=CI test', '-c',
                        'user.email=ci@example.invalid', 'commit', '--allow-empty', '--no-gpg-sign',
                        '-qm', 'fixture'], check=True)
        with socket.socket() as server:
            server.bind(('127.0.0.1', 0))
            server.listen()
            env['ELEPHC_IMAGE_TEST_PORT'] = str(server.getsockname()[1])
            def respond():
                """Respond to the real container through the host loopback interface."""
                server.settimeout(60)
                with server.accept()[0] as connection:
                    connection.sendall(b'host-network-ok')
            thread = threading.Thread(target=respond)
            thread.start()
            step = work / 'first step.sh'
            step.write_text('''python3 - <<'PY'
import os, pwd, socket, sys, platform
sys.path.insert(0, 'scripts/ci')
from build_environment import validate
assert 'ELEPHC_CI_IMAGE' not in os.environ
try:
    validate({'system':'Linux', 'glibc':'999.0'}, 'linux-x86_64' if platform.machine() == 'x86_64' else 'linux-aarch64')
except ValueError as error:
    assert 'Consumer glibc is older' in str(error)
else:
    raise AssertionError('Image consumer ABI check was bypassed')
assert pwd.getpwuid(os.getuid()).pw_name == os.environ["ELEPHC_IMAGE_TEST_USER"]
assert os.environ['ELEPHC_IMAGE_TEST_SECRET'] == 'first\\nsecond'
with socket.create_connection(('127.0.0.1', int(os.environ['ELEPHC_IMAGE_TEST_PORT']))) as connection:
    assert connection.recv(64) == b'host-network-ok'
with open(os.environ['GITHUB_ENV'], 'a') as output:
    output.write('PERSISTED=ok\\n')
PY
git -C "$RUNNER_TEMP/checkout" rev-parse HEAD
cargo --version
cargo metadata --no-deps --format-version 1 --offline > "$RUNNER_TEMP/metadata.json"
''')
            subprocess.run(['python3', '-B', 'scripts/ci/image_shell.py', str(step)], env=env, check=True)
            thread.join()
        assert (work / 'github-env').read_text() == 'PERSISTED=ok\n'
        assert json.loads((work / 'metadata.json').read_text())['workspace_members']
        guard = work / 'elephc-consumer-bin'
        guard.mkdir()
        shutil_source = ROOT / 'scripts/ci/consumer_cargo.sh'
        (guard / 'cargo').write_bytes(shutil_source.read_bytes())
        (guard / 'cargo').chmod(0o755)
        env['ELEPHC_CI_BUILD_GUARD_LOG'] = str(work / 'guard.log')
        env['ELEPHC_CI_REAL_CARGO'] = '/host-only/cargo'
        step.write_text('cargo --version\nif cargo build; then exit 1; else test $? = 86; fi\n')
        subprocess.run(['python3', '-B', 'scripts/ci/image_shell.py', str(step)], env=env, check=True)
        assert 'Unexpected cargo build' in (work / 'guard.log').read_text()
    print('Image execution boundary passed: network, paths, ownership, metadata, and build audit')


if __name__ == '__main__':
    main()
