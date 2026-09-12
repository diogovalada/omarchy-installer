import { execFileSync } from 'node:child_process';
import { appendFileSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

// Unknown paths and unavailable history run the full suite. Images and other
// assets under docs can be build inputs, so only prose is exempted.
export function docsOnly(paths) {
  return paths.length > 0 && paths.every(path =>
    !path.startsWith('.github/') &&
    (path === 'LICENSE' || /\.(md|rst|adoc)$/i.test(path) ||
      (path.startsWith('docs/') && path.endsWith('.txt'))));
}

export function changedFiles(eventName, event, git) {
  let base;
  let head;
  if (eventName === 'pull_request') {
    base = event.pull_request.base.sha;
    head = event.pull_request.head.sha;
  } else if (eventName === 'push') {
    base = event.before;
    head = event.after;
  } else {
    return null;
  }
  if (![base, head].every(sha => /^[a-f0-9]{40,64}$/.test(sha) && !/^0+$/.test(sha))) return null;
  try {
    if (eventName === 'pull_request') base = git(['merge-base', base, head]).trim();
    // Disable rename detection: moving code to a prose path must still count
    // as a code deletion. NUL delimiters preserve spaces/newlines in filenames.
    return git(['diff', '--name-only', '--no-renames', '-z', base, head, '--']).split('\0').filter(Boolean);
  } catch {
    return null;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
  const paths = changedFiles(process.env.GITHUB_EVENT_NAME, event,
    args => execFileSync('git', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }));
  const codeChanged = paths === null || !docsOnly(paths);
  appendFileSync(process.env.GITHUB_OUTPUT, `code_changed=${codeChanged}\n`);
  console.log(codeChanged ? 'Run the full suite: code changed or history is unavailable.' : 'Documentation-only change: run lightweight checks.');
}
