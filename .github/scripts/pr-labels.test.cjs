'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const {
  classifyPullRequest,
  inferSize,
  inferTargets,
  inferType,
  parseSelection,
  reconcileLabels,
  run,
  selectAreas,
  validateConfig,
} = require('./pr-labels.cjs');

test('the label configuration is internally complete', () => {
  assert.doesNotThrow(validateConfig);
});

test('title type takes precedence over a conflicting branch prefix', () => {
  assert.equal(inferType('fix(types): repair array widening', 'feat/array-widening'), 'type:fix');
});

test('branch type classifies a non-conventional title', () => {
  assert.equal(inferType('Magician', 'feat/eval'), 'type:feature');
});

test('unclassified titles and branches receive the triage type', () => {
  assert.equal(inferType('Improve the compiler', 'work/compiler'), 'type:triage');
});

test('docs-only changes do not acquire source-code areas', () => {
  const result = selectAreas(['README.md', 'docs/php/functions.md', '.plans/next-step.md']);
  assert.deepEqual(result.labels, ['area:docs']);
});

test('distinctive domains are retained while areas are capped', () => {
  const result = classifyPullRequest({
    title: 'feat(eval): add dynamic execution',
    headBranch: 'feat/eval',
    changedFiles: 8,
    additions: 600,
    deletions: 20,
    files: [
      'crates/elephc-magician/src/lib.rs',
      'crates/elephc-magician/src/interpreter.rs',
      'src/types/checker/functions.rs',
      'src/types/call_args/mod.rs',
      'src/ir_lower/expr/mod.rs',
      'src/codegen/lower_inst/calls.rs',
      'src/parser/expr/pratt.rs',
      'docs/php/eval.md',
    ],
  });

  assert.equal(result.labels.filter((label) => label.startsWith('area:')).length, 3);
  assert.ok(result.labels.includes('area:magician'));
  assert.ok(result.labels.includes('scope:multi-area'));
});

test('incidental platform and builtin files do not displace Magician primary areas', () => {
  const result = classifyPullRequest({
    title: 'Magician',
    headBranch: 'feat/eval',
    changedFiles: 12,
    additions: 1_000,
    deletions: 50,
    files: [
      'crates/elephc-magician/src/lib.rs',
      'crates/elephc-magician/src/interpreter/mod.rs',
      'crates/elephc-magician/src/interpreter/eval.rs',
      'src/types/checker/functions.rs',
      'src/types/call_args/mod.rs',
      'src/optimize/fold/calls.rs',
      'src/optimize/effects/calls.rs',
      'src/linker.rs',
      'src/builtins/internal/eval.rs',
      'docs/php/eval.md',
      'tests/codegen/eval.rs',
      'README.md',
    ],
  });

  assert.deepEqual(
    result.labels.filter((label) => label.startsWith('area:')),
    ['area:magician', 'area:types', 'area:optimizer'],
  );
});

test('Windows x86_64 changes do not acquire the Linux x86_64 target', () => {
  assert.deepEqual(
    inferTargets({
      title: 'feat(platform): add Windows x86_64 PE32+ support',
      headBranch: 'feat/windows-pe',
      files: ['src/codegen_support/windows_x86_64.rs'],
    }),
    ['target:windows-x86_64'],
  );
});

test('a generic x86_64 backend fix maps to the supported Linux target', () => {
  assert.deepEqual(
    inferTargets({
      title: 'fix(codegen): use the canonical x86_64 heap magic',
      headBranch: 'fix/canonical-heap-magic',
      files: ['src/codegen_support/runtime/heap.rs'],
    }),
    ['target:linux-x86_64'],
  );
});

test('a target-specific PR keeps platform among its primary areas', () => {
  const result = classifyPullRequest({
    title: 'feat(platform): add Windows x86_64 PE32+ support',
    headBranch: 'feat/windows-pe',
    changedFiles: 5,
    additions: 400,
    deletions: 20,
    files: [
      'src/codegen_support/windows_x86_64.rs',
      'src/codegen/lower_inst/calls.rs',
      'src/codegen/lower_inst/arrays.rs',
      'src/codegen_support/runtime/mod.rs',
      'src/linker.rs',
    ],
  });

  assert.ok(result.labels.includes('area:platform'));
  assert.ok(result.labels.includes('target:windows-x86_64'));
});

test('changes naming multiple supported targets are treated as cross-target', () => {
  assert.deepEqual(
    inferTargets({
      title: 'chore(ci): update target jobs',
      headBranch: 'chore/targets',
      files: ['config/linux-x86_64.toml', 'config/linux-aarch64.toml'],
    }),
    [],
  );
});

test('size thresholds use both changed files and changed lines', () => {
  assert.equal(inferSize({ changedFiles: 2, additions: 20, deletions: 5 }), 'size:xs');
  assert.equal(inferSize({ changedFiles: 5, additions: 90, deletions: 20 }), 'size:s');
  assert.equal(inferSize({ changedFiles: 12, additions: 300, deletions: 20 }), 'size:m');
  assert.equal(inferSize({ changedFiles: 31, additions: 100, deletions: 10 }), 'size:l');
  assert.equal(inferSize({ changedFiles: 101, additions: 100, deletions: 10 }), 'size:xl');
});

test('managed labels synchronize while manual topics remain untouched', () => {
  assert.deepEqual(
    reconcileLabels(
      ['type:feature', 'area:parser', 'size:s', 'topic:php-compat', 'help wanted'],
      ['type:fix', 'area:types', 'size:m'],
    ),
    {
      add: ['type:fix', 'area:types', 'size:m'],
      remove: ['type:feature', 'area:parser', 'size:s'],
    },
  );
});

test('manual selection accepts unique PR numbers and all-open', () => {
  assert.deepEqual(parseSelection('535, 533 535'), [535, 533]);
  assert.equal(parseSelection('all-open'), 'all-open');
  assert.throws(() => parseSelection('535,nope'), /Invalid PR selection/);
});

test('workflow dry-run reports changes without calling write endpoints', async () => {
  const writes = [];
  const github = {
    rest: {
      issues: {
        listLabelsForRepo: async () => {},
        listLabelsOnIssue: async () => {},
        createLabel: async (args) => writes.push(['create', args]),
        removeLabel: async (args) => writes.push(['remove', args]),
        addLabels: async (args) => writes.push(['add', args]),
      },
      pulls: {
        list: async () => {},
        get: async () => {},
        listFiles: async () => {},
      },
    },
  };
  github.paginate = async (method) => {
    if (method === github.rest.issues.listLabelsForRepo) return [];
    if (method === github.rest.issues.listLabelsOnIssue) {
      return [{ name: 'topic:ownership-gc' }, { name: 'type:feature' }];
    }
    if (method === github.rest.pulls.listFiles) {
      return [{ filename: 'src/ir_lower/ownership.rs' }];
    }
    throw new Error('Unexpected pagination endpoint');
  };
  const messages = [];
  const context = {
    repo: { owner: 'illegalstudio', repo: 'elephc' },
    payload: {
      pull_request: {
        number: 535,
        title: 'fix(ir): release loop-carried locals',
        head: { ref: 'fix/loop-carry-release' },
        changed_files: 1,
        additions: 20,
        deletions: 2,
      },
    },
  };

  const results = await run({
    github,
    context,
    core: { info: (message) => messages.push(message) },
    dryRun: true,
  });

  assert.deepEqual(writes, []);
  assert.deepEqual(results[0].labels, ['type:fix', 'area:eir', 'size:xs']);
  assert.deepEqual(results[0].remove, ['type:feature']);
  assert.ok(messages.some((message) => message.includes('Would create labels:')));
});
