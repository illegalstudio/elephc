"""Check the container boundary that preserves host database services and artifact paths."""

import os
import json
import subprocess
import sys
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from image_shell import image_ref, install, invocation, main

ROOT = Path(__file__).resolve().parents[2]
IMAGE = 'ghcr.io/illegalstudio/elephc-ci@sha256:' + '1' * 64


class ImageExecution(unittest.TestCase):
    """Verify real command construction without network access or compilation."""

    def setUp(self):
        """Keep every fixture beneath one task-owned directory."""
        directory = tempfile.TemporaryDirectory(prefix='.image-shell-test.', dir=ROOT)
        self.addCleanup(directory.cleanup)
        self.temporary = Path(directory.name)
        self.script = self.temporary / 'step with spaces.sh'
        self.script.write_text('true\n')
        self.env = {'GITHUB_WORKSPACE': str(ROOT), 'RUNNER_TEMP': str(self.temporary),
                    'ELEPHC_CI_EXEC_IMAGE': IMAGE, 'ELEPHC_CI_EXEC_ROLE': 'smoke',
                    'HOME': '/host/home', 'PATH': '/host/only', 'SECRET': 'first\nsecond',
                    'ELEPHC_CI_IMAGE': 'producer-only-identity'}

    def test_pinned_image_host_network_and_same_path_mounts(self):
        """The consumer sees the service localhost, generated script and shared artifact tree."""
        args = invocation(self.script, self.env)
        self.assertIn('--network=host', args)
        self.assertEqual(args[args.index('--user') + 1], f'{os.getuid()}:{os.getgid()}')
        self.assertIn(f'{ROOT}:{ROOT}', args)
        self.assertIn(f'{self.temporary}:{self.temporary}', args)
        self.assertEqual(args[-1], str(self.script))
        self.assertEqual(args[args.index(IMAGE) + 1:args.index(IMAGE) + 3], ['bash', '--noprofile'])

    def test_step_environment_is_forwarded_without_secret_values_in_arguments(self):
        """Docker reads fresh values by name, preserving multiline secrets and env-file updates."""
        args = invocation(self.script, self.env)
        self.assertIn('SECRET', args)
        self.assertNotIn(self.env['SECRET'], args)
        self.assertNotIn('ELEPHC_CI_IMAGE', args)
        self.assertFalse(any('PATH=/host/only' in arg for arg in args))
        self.assertIn('RUSTUP_HOME=/usr/local/rustup', args)

    def test_consumer_guard_uses_baked_cargo_and_remains_first_in_path(self):
        """A host Rust path must not bypass the existing no-rebuild audit inside an image."""
        self.env.update(ELEPHC_CI_BUILD_GUARD_LOG='/guard.log', ELEPHC_CI_REAL_CARGO='/host/cargo')
        args = invocation(self.script, self.env)
        self.assertIn('ELEPHC_CI_REAL_CARGO=/usr/local/cargo/bin/cargo', args)
        self.assertNotIn('/host/cargo', args)
        self.assertTrue(next(arg for arg in args if arg.startswith('PATH=')).startswith(
            f'PATH={self.temporary}/elephc-consumer-bin:'))

    def test_php_uses_selected_preinstalled_sdk(self):
        """NTS and ZTS images expose their release SDK while keeping the debug SDK separate."""
        self.env['ELEPHC_CI_EXEC_ROLE'] = 'php-ts'
        args = invocation(self.script, self.env)
        self.assertTrue(next(arg for arg in args if arg.startswith('PATH=')).startswith('PATH=/opt/elephc/php/bin:'))

    def test_installed_shell_runs_from_a_subdirectory_via_the_actions_path(self):
        """Resolve the same custom shell as Actions while PHPUnit starts in examples/testing."""
        path_file = self.temporary / 'github-path'
        env = dict(os.environ, RUNNER_TEMP=str(self.temporary), GITHUB_PATH=str(path_file))
        with patch.dict(os.environ, env):
            install()
        directory = Path(path_file.read_text().strip())
        self.assertTrue((directory / 'elephc-ci-shell').is_file())
        fake = self.temporary / 'docker'
        fake.write_text('#!/usr/bin/env python3\nimport json,os,sys\nprint(json.dumps({"cwd":os.getcwd(),"args":sys.argv[1:]}))\n')
        fake.chmod(0o755)
        env.update(self.env, PATH=str(directory) + ':' + str(self.temporary) + ':' + os.environ['PATH'])
        project = ROOT / 'examples/testing'
        self.script.write_text("python3 -c 'import json,os;print(json.dumps({\"cwd\":os.getcwd(),\"native\":True}))'\n")
        result = subprocess.check_output(['elephc-ci-shell', str(self.script)], cwd=project, env=env, text=True)
        execution = json.loads(result)
        self.assertEqual(execution['cwd'], str(project))
        if sys.platform == 'darwin':
            self.assertTrue(execution['native'])
        else:
            self.assertEqual(execution['args'][execution['args'].index('--workdir') + 1], str(project))

    def test_macos_dispatch_keeps_the_native_shell(self):
        """macOS must not enter Linux image construction, even when tested on Linux."""
        with patch('image_shell.sys.platform', 'darwin'), \
             patch('image_shell.sys.argv', ['elephc-ci-shell', str(self.script)]), \
             patch('image_shell.invocation', side_effect=AssertionError('Unexpected Docker on macOS')), \
             patch('image_shell.os.execvp') as execute:
            main()
        self.assertEqual(execute.call_args.args[0], 'bash')
        self.assertEqual(execute.call_args.args[1][-1], str(self.script))

    def test_workflow_shell_entry_points_resolve_without_github_expressions(self):
        """GitHub shell fields accept commands from PATH but no expression contexts."""
        for workflow in (ROOT / '.github/workflows').glob('*.yml'):
            definition = json.loads(subprocess.check_output([
                'ruby', '-ryaml', '-rjson', '-e', 'puts JSON.generate(YAML.load_file(ARGV[0]))', str(workflow)
            ], text=True))
            for job in definition.get('jobs', {}).values():
                for step in job.get('steps', []):
                    shell = step.get('shell', '')
                    with self.subTest(workflow=workflow.name, step=step.get('name')):
                        self.assertNotIn('${{', shell)
                        self.assertNotIn('scripts/ci/image_shell.py', shell)

    def test_mutable_or_foreign_images_and_unmounted_scripts_are_rejected(self):
        """Missing image publication never falls back to the host or a moving tag."""
        for image in ('ghcr.io/illegalstudio/elephc-ci:latest', 'evil.example/ci@sha256:' + '1' * 64, ''):
            with self.assertRaises(ValueError):
                image_ref(image)
        with self.assertRaises(ValueError):
            invocation('/unmounted/step.sh', self.env)


if __name__ == '__main__':
    unittest.main()
