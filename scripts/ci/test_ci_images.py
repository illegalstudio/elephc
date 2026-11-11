"""Prevent image publication or mutable tags from changing an in-flight CI environment."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
IMAGE = 'ghcr.io/illegalstudio/elephc-ci@sha256:' + '1' * 64


class CommittedImageResolution(unittest.TestCase):
    """Run the production resolver with an isolated catalog and fake registry client."""

    def setUp(self):
        """Put fake executables and outputs in one directory that is always cleaned."""
        tmp = tempfile.TemporaryDirectory(prefix='.ci-images-test.', dir=ROOT)
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        (self.root / 'scripts/ci').mkdir(parents=True)
        docker = self.root / 'docker'
        docker.write_text('''#!/bin/sh
printf '%s\\n' "$*" >> "$DOCKER_CALLS"
case "$*" in
  *:latest*) printf 'sha256:%064d\\n' 2 ;;
  *imagetools*) exit "${REGISTRY_FAILURE:-0}" ;;
esac
''')
        docker.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.root) + ':' + os.environ['PATH'],
                        GITHUB_OUTPUT=str(self.root / 'output'), DOCKER_CALLS=str(self.root / 'calls'))
        self.env.pop('GH_TOKEN', None)

    def execute(self, image=None):
        """Execute Bash directly so its fail-closed branches are actually exercised."""
        if image is not None:
            (self.root / 'scripts/ci/images.json').write_text(json.dumps({'ci': image}))
        return subprocess.run(['bash', str(ROOT / 'scripts/ci/resolve_ci_image.sh')],
                              cwd=self.root, env=self.env, capture_output=True, text=True)

    def test_catalog_pin_wins_over_newer_latest_publication(self):
        """A newly pushed latest image cannot change the producer/shard digest."""
        self.assertEqual(self.execute(IMAGE).returncode, 0)
        self.assertEqual((self.root / 'output').read_text(), f'image={IMAGE}\n')
        self.assertNotIn(':latest', (self.root / 'calls').read_text())

    def test_invalid_or_unpublished_catalog_never_falls_back(self):
        """A malformed pin and a missing image must fail before emitting a producer identity."""
        self.assertNotEqual(self.execute('ghcr.io/illegalstudio/elephc-ci:latest').returncode, 0)
        self.assertFalse((self.root / 'output').exists())
        self.env['REGISTRY_FAILURE'] = '17'
        self.assertEqual(self.execute(IMAGE).returncode, 17)
        self.assertFalse((self.root / 'output').exists())
        self.assertNotIn(':latest', (self.root / 'calls').read_text())

    def test_historical_workflow_without_catalog_resolves_an_immutable_digest(self):
        """Previously retained source commits continue to work without the new catalog."""
        self.assertEqual(self.execute().returncode, 0)
        self.assertEqual((self.root / 'output').read_text(),
                         'image=ghcr.io/illegalstudio/elephc-ci@sha256:' + '0' * 63 + '2\n')


class ImagePublication(unittest.TestCase):
    """Keep successful architecture tags usable when only a failed build is rerun."""

    def test_manifest_combines_retained_success_with_failed_architecture_rerun(self):
        """Execute the real publisher against a registry containing mixed attempt numbers."""
        definition = json.loads(subprocess.check_output([
            'ruby', '-ryaml', '-rjson', '-e', 'puts JSON.generate(YAML.load_file(ARGV[0]))',
            str(ROOT / '.github/workflows/ci-image.yml')], text=True))
        build = next(step for step in definition['jobs']['build']['steps']
                     if 'docker/build-push-action@' in step.get('uses', ''))
        publish = next(step['run'] for step in definition['jobs']['manifest']['steps'] if 'run' in step)
        with tempfile.TemporaryDirectory(prefix='.image-publication-test.', dir=ROOT) as directory:
            root = Path(directory)
            registry = root / 'registry.json'
            variants = definition['jobs']['build']['strategy']['matrix']['include']
            retained = []
            for variant in variants:
                failed = variant['role'] == 'ci' and variant['arch'] == 'amd64'
                values = {'env.IMAGE': 'ghcr.io/illegalstudio/elephc-ci', 'github.run_id': '123',
                          'github.run_attempt': '2' if failed else '1',
                          'matrix.role': variant['role'], 'matrix.arch': variant['arch']}
                tag = build['with']['tags']
                for key, value in values.items():
                    tag = tag.replace('${{ ' + key + ' }}', value)
                retained.append(tag)
            registry.write_text(json.dumps(retained))
            docker = root / 'docker'
            docker.write_text('''#!/usr/bin/env python3
import json,os,sys
from pathlib import Path
path=Path(os.environ['FAKE_REGISTRY'])
known=set(json.loads(path.read_text()))
args=sys.argv[1:]
if args[:3]==['buildx','imagetools','create']:
    inputs=[];tags=[];index=3
    while index<len(args):
        if args[index]=='--tag':tags.append(args[index+1]);index+=2
        else:inputs.append(args[index]);index+=1
    missing=[tag for tag in inputs if tag not in known]
    if missing:
        print('Image not published: '+str(missing),file=sys.stderr);sys.exit(44)
    path.write_text(json.dumps(sorted(known|set(tags))))
elif args[:3]==['buildx','imagetools','inspect']:
    assert args[3] in known
else:raise SystemExit('Unexpected Docker command')
''')
            docker.chmod(0o755)
            env = dict(os.environ, PATH=str(root) + ':' + os.environ['PATH'],
                       FAKE_REGISTRY=str(registry), IMAGE='ghcr.io/illegalstudio/elephc-ci', ROLE='ci',
                       GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT='2', GITHUB_SHA='a' * 40,
                       GITHUB_REF='refs/heads/candidate')
            result = subprocess.run(['bash', '-e', '-o', 'pipefail', '-c', publish],
                                    env=env, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == '__main__':
    unittest.main()
