import assert from 'node:assert/strict';
import { test } from 'node:test';
import { changedFiles, docsOnly } from './ci-changes.mjs';

test('only prose changes skip heavy jobs', () => {
  assert.equal(docsOnly(['README.md', 'docs/design.md', 'HANDOFF.md', 'LICENSE']), true);
  for (const path of ['docs/images/installer-start.jpg', 'docs/fixture.json', 'Cargo.lock', '.github/README.md', 'scripts/build.mjs']) {
    assert.equal(docsOnly(['README.md', path]), false, path);
  }
  assert.equal(docsOnly([]), false);
});
test('push comparison covers all commits and preserves unusual filenames', () => {
  const event = { before: 'a'.repeat(40), after: 'b'.repeat(40) };
  const result = changedFiles('push', event, args => {
    assert.deepEqual(args, ['diff', '--name-only', '--no-renames', '-z', event.before, event.after, '--']);
    return 'README.md\0docs/a\nb.md\0src/removed.rs\0';
  });
  assert.equal(docsOnly(result), false);
  assert.equal(result[1], 'docs/a\nb.md');
});
test('pull requests compare against their merge base', () => {
  const event = { pull_request: { base: { sha: 'a'.repeat(40) }, head: { sha: 'b'.repeat(40) } } };
  const calls = [];
  const result = changedFiles('pull_request', event, args => {
    calls.push(args);
    return args[0] === 'merge-base' ? 'c'.repeat(40) + '\n' : 'README.md\0';
  });
  assert.equal(calls[1][4], 'c'.repeat(40));
  assert.equal(docsOnly(result), true);
});
test('manual runs, new branches and unavailable history request the full suite', () => {
  const noGit = () => { throw new Error('missing history'); };
  assert.equal(changedFiles('workflow_dispatch', {}, noGit), null);
  assert.equal(changedFiles('push', { before: '0'.repeat(40), after: 'b'.repeat(40) }, noGit), null);
  assert.equal(changedFiles('push', { before: 'a'.repeat(40), after: 'b'.repeat(40) }, noGit), null);
});
