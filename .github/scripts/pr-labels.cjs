'use strict';

/**
 * Pull-request label classification and GitHub synchronization for elephc.
 *
 * The classifier manages type, area, target, size, and scope labels. Topic and
 * priority labels are deliberately left to maintainers because their meaning
 * cannot be inferred safely from filenames alone.
 */

const fs = require('node:fs');
const path = require('node:path');

const CONFIG_PATH = path.join(__dirname, '..', 'pr-labels.json');
const config = JSON.parse(fs.readFileSync(CONFIG_PATH, 'utf8'));

const compiledAreaRules = config.areas.map(compileRule);
const compiledTargetRules = config.targets.map(compileRule);
const supportedTargets = new Set([
  'target:linux-x86_64',
  'target:linux-aarch64',
  'target:macos-aarch64',
]);

/** Compile the regular expressions declared for one label rule. */
function compileRule(rule) {
  return {
    ...rule,
    regexes: rule.patterns.map((pattern) => new RegExp(pattern, 'i')),
  };
}

/** Return a normalized repository-relative path. */
function normalizePath(file) {
  const value = typeof file === 'string' ? file : file.filename ?? file.path ?? file.name;
  return String(value ?? '').replaceAll('\\', '/').replace(/^\.\//, '');
}

/** Identify files that count as documentation for docs-only classification. */
function isDocumentationFile(file) {
  return (
    file.startsWith('docs/') ||
    file.startsWith('.plans/') ||
    /(?:^|\/)(?:README|CHANGELOG|CONTRIBUTING|AGENTS|CLAUDE)\.md$/i.test(file) ||
    /\.md$/i.test(file)
  );
}

/** Infer exactly one type label from title first and head branch second. */
function inferType(title, headBranch) {
  const titleMatch = String(title ?? '').match(
    /^(feat|feature|fix|docs|refactor|chore|tests?)(?:\([^)]*\))?!?:/i,
  );
  const branchMatch = String(headBranch ?? '').match(
    /^(feat|feature|fix|docs|refactor|chore|tests?)\//i,
  );
  const prefix = (titleMatch?.[1] ?? branchMatch?.[1] ?? '').toLowerCase();
  return config.typePrefixes[prefix] ?? 'type:triage';
}

/** Score each compiler area by the number of matching changed files. */
function scoreAreas(files) {
  const scores = new Map();

  for (const file of files) {
    for (const rule of compiledAreaRules) {
      if (rule.regexes.some((regex) => regex.test(file))) {
        scores.set(rule.label, (scores.get(rule.label) ?? 0) + 1);
      }
    }
  }

  return scores;
}

/** Select up to the configured number of primary areas from scored matches. */
function selectAreas(files, preferredAreas = []) {
  if (files.length > 0 && files.every(isDocumentationFile)) {
    return {
      labels: ['area:docs'],
      matchedAreaCount: 1,
      scores: new Map([['area:docs', files.length]]),
    };
  }

  const scores = scoreAreas(files);
  const order = new Map(config.areas.map((rule, index) => [rule.label, index]));
  const ranked = [...scores.entries()].sort(
    ([leftLabel, leftScore], [rightLabel, rightScore]) =>
      rightScore - leftScore || order.get(leftLabel) - order.get(rightLabel),
  );
  const selected = [];

  for (const label of [...config.domainAreas, ...preferredAreas]) {
    if (
      scores.has(label) &&
      !selected.includes(label) &&
      selected.length < config.maxAreaLabels
    ) {
      selected.push(label);
    }
  }

  for (const [label] of ranked) {
    if (!selected.includes(label) && selected.length < config.maxAreaLabels) {
      selected.push(label);
    }
  }

  return {
    labels: selected.length > 0 ? selected : ['area:triage'],
    matchedAreaCount: scores.size,
    scores,
  };
}

/** Infer area preferences from conventional title scopes and target metadata. */
function inferPreferredAreas({ title, headBranch, targets }) {
  const scopeMap = {
    lexer: 'area:lexer',
    parser: 'area:parser',
    resolver: 'area:resolver',
    types: 'area:types',
    type: 'area:types',
    optimizer: 'area:optimizer',
    optimize: 'area:optimizer',
    ir: 'area:eir',
    eir: 'area:eir',
    ir_lower: 'area:eir',
    codegen: 'area:codegen',
    runtime: 'area:runtime',
    builtins: 'area:builtins',
    builtin: 'area:builtins',
    web: 'area:web',
    platform: 'area:platform',
    ci: 'area:tooling-ci',
    tooling: 'area:tooling-ci',
    docs: 'area:docs',
  };
  const preferred = [];
  const titleScope = String(title ?? '').match(
    /^(?:feat|feature|fix|docs|refactor|chore|tests?)\(([^)]+)\)!?:/i,
  )?.[1];

  for (const scope of String(titleScope ?? '').toLowerCase().split(/[,|/]+/)) {
    if (scopeMap[scope.trim()]) preferred.push(scopeMap[scope.trim()]);
  }

  const branch = String(headBranch ?? '').toLowerCase();
  if (/^(?:feat|feature|fix)\/(?:eval|magician)(?:[/_-]|$)/.test(branch)) {
    preferred.push('area:magician');
  }
  if (/^(?:feat|feature|fix)\/web(?:[/_-]|$)/.test(branch)) {
    preferred.push('area:web');
  }
  if (targets.length > 0) preferred.push('area:platform');

  return [...new Set(preferred)];
}

/** Infer target-specific labels without labeling an all-supported-target change. */
function inferTargets({ title, headBranch, files }) {
  const searchable = [title, headBranch, ...files].join('\n');
  const matched = [];

  for (const rule of compiledTargetRules) {
    if (rule.regexes.some((regex) => regex.test(searchable))) {
      matched.push(rule.label);
    }
  }

  const filtered = matched.filter((label) => {
    const rule = config.targets.find((candidate) => candidate.label === label);
    return !(rule.unless ?? []).some((excludedBy) => matched.includes(excludedBy));
  });
  const matchedSupportedTargets = filtered.filter((label) => supportedTargets.has(label));

  if (matchedSupportedTargets.length > 1) {
    return filtered.filter((label) => !supportedTargets.has(label));
  }

  return filtered;
}

/** Infer one review-size label from changed-file and changed-line counts. */
function inferSize({ changedFiles, additions, deletions }) {
  const files = Number(changedFiles) || 0;
  const lines = (Number(additions) || 0) + (Number(deletions) || 0);

  if (files > 100 || lines > 10_000) return 'size:xl';
  if (files > 30 || lines > 2_000) return 'size:l';
  if (files > 10 || lines > 500) return 'size:m';
  if (files > 3 || lines > 100) return 'size:s';
  return 'size:xs';
}

/** Classify a pull request from metadata and its complete changed-file list. */
function classifyPullRequest(pullRequest) {
  const files = (pullRequest.files ?? []).map(normalizePath).filter(Boolean);
  const title = pullRequest.title ?? '';
  const headBranch =
    pullRequest.headBranch ?? pullRequest.headRefName ?? pullRequest.head?.ref ?? '';
  const targets = inferTargets({ title, headBranch, files });
  const preferredAreas = inferPreferredAreas({ title, headBranch, targets });
  const areas = selectAreas(files, preferredAreas);
  const labels = [
    inferType(title, headBranch),
    ...areas.labels,
    ...targets,
    inferSize(pullRequest),
  ];

  if (areas.matchedAreaCount > config.maxAreaLabels || Number(pullRequest.changedFiles) > 100) {
    labels.push('scope:multi-area');
  }

  return {
    labels: [...new Set(labels)],
    areaScores: Object.fromEntries(areas.scores),
  };
}

/** Return whether a label belongs to a namespace managed by the automation. */
function isManagedLabel(label) {
  return config.managedPrefixes.some((prefix) => label.startsWith(prefix));
}

/** Calculate managed-label additions/removals while preserving manual labels. */
function reconcileLabels(currentLabels, desiredLabels) {
  const current = new Set(currentLabels);
  const desired = new Set(desiredLabels);
  return {
    add: [...desired].filter((label) => !current.has(label)),
    remove: [...current].filter((label) => isManagedLabel(label) && !desired.has(label)),
  };
}

/** Parse comma/space separated PR numbers or the special all-open selection. */
function parseSelection(selection) {
  const value = String(selection ?? '').trim().toLowerCase();
  if (value === 'all-open') return value;
  if (value === '') return [];

  const numbers = value
    .split(/[\s,]+/)
    .filter(Boolean)
    .map((part) => Number(part));
  if (numbers.some((number) => !Number.isSafeInteger(number) || number <= 0)) {
    throw new Error(`Invalid PR selection: ${selection}`);
  }
  return [...new Set(numbers)];
}

/** Validate that every classifier output has a catalog definition. */
function validateConfig() {
  if (!Number.isInteger(config.maxAreaLabels) || config.maxAreaLabels < 1) {
    throw new Error('maxAreaLabels must be a positive integer.');
  }

  for (const [name, definition] of Object.entries(config.labels)) {
    if (name.length > 50) throw new Error(`Label name exceeds 50 characters: ${name}`);
    if (!/^[0-9a-f]{6}$/i.test(definition.color)) {
      throw new Error(`Invalid label color for ${name}: ${definition.color}`);
    }
    if (definition.description.length > 100) {
      throw new Error(`Label description exceeds 100 characters: ${name}`);
    }
  }

  const areaRuleLabels = config.areas.map((rule) => rule.label);
  if (new Set(areaRuleLabels).size !== areaRuleLabels.length) {
    throw new Error('Area rules must use unique labels.');
  }
  const unknownDomainAreas = config.domainAreas.filter(
    (label) => !areaRuleLabels.includes(label),
  );
  if (unknownDomainAreas.length > 0) {
    throw new Error(`Unknown domain areas: ${unknownDomainAreas.join(', ')}`);
  }

  const referenced = new Set([
    ...Object.values(config.typePrefixes),
    'type:triage',
    'area:docs',
    'area:triage',
    'scope:multi-area',
    'size:xs',
    'size:s',
    'size:m',
    'size:l',
    'size:xl',
    ...config.areas.map((rule) => rule.label),
    ...config.targets.map((rule) => rule.label),
  ]);
  const missing = [...referenced].filter((label) => !config.labels[label]);
  if (missing.length > 0) {
    throw new Error(`Missing label definitions: ${missing.join(', ')}`);
  }
}

/** Ensure the declarative label catalog exists in the GitHub repository. */
async function ensureLabelCatalog({ github, owner, repo, core, dryRun }) {
  const repositoryLabels = await github.paginate(github.rest.issues.listLabelsForRepo, {
    owner,
    repo,
    per_page: 100,
  });
  const existing = new Set(repositoryLabels.map((label) => label.name));
  const missing = Object.entries(config.labels).filter(([name]) => !existing.has(name));

  if (dryRun) {
    if (missing.length > 0) {
      core.info(`Would create labels: ${missing.map(([name]) => name).join(', ')}`);
    }
    return;
  }

  for (const [name, definition] of missing) {
    try {
      await github.rest.issues.createLabel({ owner, repo, name, ...definition });
    } catch (error) {
      if (error.status !== 422) throw error;
    }
  }
}

/** Resolve the PRs selected by an event number, explicit numbers, or all-open. */
async function resolvePullRequests({ github, owner, repo, context, selection }) {
  const parsed = parseSelection(selection);
  if (parsed === 'all-open') {
    return github.paginate(github.rest.pulls.list, {
      owner,
      repo,
      state: 'open',
      per_page: 100,
    });
  }

  const eventPullRequest = context.payload.pull_request;
  const numbers = parsed.length > 0 ? parsed : eventPullRequest ? [eventPullRequest.number] : [];
  if (numbers.length === 0) {
    throw new Error('No pull request number was supplied.');
  }

  return Promise.all(
    numbers.map(async (number) => {
      if (eventPullRequest?.number === number) return eventPullRequest;
      const response = await github.rest.pulls.get({ owner, repo, pull_number: number });
      return response.data;
    }),
  );
}

/** Fetch every changed filename for one pull request. */
async function fetchChangedFiles({ github, owner, repo, number }) {
  return github.paginate(github.rest.pulls.listFiles, {
    owner,
    repo,
    pull_number: number,
    per_page: 100,
  });
}

/** Synchronize labels for one pull request, or only report the planned changes. */
async function labelPullRequest({ github, owner, repo, pullRequest, core, dryRun }) {
  const number = pullRequest.number;
  const files = await fetchChangedFiles({ github, owner, repo, number });
  let details = pullRequest;
  if (
    details.changed_files == null ||
    details.additions == null ||
    details.deletions == null
  ) {
    const response = await github.rest.pulls.get({ owner, repo, pull_number: number });
    details = response.data;
  }
  const classification = classifyPullRequest({
    title: details.title,
    headBranch: details.head?.ref,
    changedFiles: details.changed_files ?? files.length,
    additions: details.additions,
    deletions: details.deletions,
    files,
  });
  const current = await github.paginate(github.rest.issues.listLabelsOnIssue, {
    owner,
    repo,
    issue_number: number,
    per_page: 100,
  });
  const changes = reconcileLabels(
    current.map((label) => label.name),
    classification.labels,
  );
  const summary = `#${number}: ${classification.labels.join(', ')}`;

  if (dryRun) {
    core.info(`${summary}; add=[${changes.add.join(', ')}]; remove=[${changes.remove.join(', ')}]`);
    return { number, ...classification, ...changes };
  }

  for (const label of changes.remove) {
    try {
      await github.rest.issues.removeLabel({
        owner,
        repo,
        issue_number: number,
        name: label,
      });
    } catch (error) {
      if (error.status !== 404) throw error;
    }
  }

  if (changes.add.length > 0) {
    await github.rest.issues.addLabels({
      owner,
      repo,
      issue_number: number,
      labels: changes.add,
    });
  }

  core.info(summary);
  return { number, ...classification, ...changes };
}

/** Run PR label synchronization from a GitHub Actions event or manual dispatch. */
async function run({ github, context, core, selection = '', dryRun = false }) {
  validateConfig();
  const { owner, repo } = context.repo;
  const pullRequests = await resolvePullRequests({
    github,
    owner,
    repo,
    context,
    selection,
  });

  await ensureLabelCatalog({ github, owner, repo, core, dryRun });
  const results = [];
  for (const pullRequest of pullRequests) {
    results.push(
      await labelPullRequest({ github, owner, repo, pullRequest, core, dryRun }),
    );
  }
  return results;
}

validateConfig();

module.exports = {
  classifyPullRequest,
  config,
  inferSize,
  inferPreferredAreas,
  inferTargets,
  inferType,
  isDocumentationFile,
  parseSelection,
  reconcileLabels,
  run,
  selectAreas,
  validateConfig,
};
