// Screenshots and verbosity measurements for the "Install without USB" fixture.
// Read-only: drives test-fixtures/staged-partitions.html in headless Edge.
// Needs a running dev server (pnpm --filter @omarchy-setup/desktop dev) and
// playwright-core installed outside the workspace. In PowerShell:
//   npm install --prefix "$env:TEMP\pw-shot" playwright-core
//   $env:PLAYWRIGHT_CORE="$env:TEMP\pw-shot\node_modules\playwright-core"
//   node scripts/staged-screenshots.cjs [label] [base-url]
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const [label = 'current', base = 'http://localhost:1420'] = process.argv.slice(2);
const scenarios = ['ready', 'limited', 'oem', 'free', 'two-ntfs', 'two-disks', 'failed', 'replace-only', 'checking', 'review', 'layout=example', 'layout=laptop'];
const widths = [1180, 820, 390];
const output = path.join(__dirname, '..', 'artifacts', 'staged-disk-map', label);

// Words and size mentions in the whole visible section (navigation included;
// closed disclosures don't count). Height to button runs from the first
// section heading to the last primary button.
function measure() {
  const root = document.querySelector('.staged');
  if (!root) return null;
  const text = root.innerText;
  const visible = [...root.querySelectorAll('h3, button.primary')].filter(el => el.offsetParent);
  const heading = visible.find(el => el.tagName === 'H3');
  const button = visible.filter(el => el.matches('button.primary')).at(-1);
  return {
    words: text.split(/\s+/).filter(Boolean).length,
    sizes: (text.match(/\d+(?:\.\d+)?\s?(?:MiB|GiB|TiB|MB|GB|TB)\b/g) ?? []).length,
    height: Math.round(root.getBoundingClientRect().height),
    headingToButton: heading && button ? Math.round(button.getBoundingClientRect().bottom - heading.getBoundingClientRect().top) : null,
    overflow: document.documentElement.scrollWidth > innerWidth
  };
}

(async () => {
  fs.mkdirSync(output, { recursive: true });
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  const page = await browser.newPage({ viewport: { width: widths[0], height: 790 } });
  let errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const rows = [], failures = [];
  for (const scenario of scenarios) {
    errors = [];
    try {
      await page.setViewportSize({ width: widths[0], height: 790 });
      await page.goto(`${base}/test-fixtures/staged-partitions.html?${scenario}`);
      await page.waitForFunction(() => window.__fixtureSettled === true, null, { timeout: 15000 });
      await page.waitForTimeout(200);
      for (const width of widths) {
        await page.setViewportSize({ width, height: 790 });
        await page.waitForTimeout(100);
        await page.screenshot({ path: path.join(output, `${scenario.replace('=', '-')}-${width}.png`), fullPage: true });
        const row = await page.evaluate(measure);
        if (!row) throw new Error('the installer section did not render');
        rows.push({ scenario, width, ...row });
        if (row.overflow) failures.push(`${scenario}: horizontal overflow at ${width}px`);
      }
    } catch (error) {
      failures.push(`${scenario}: ${error.message.split('\n')[0]}`);
    }
    failures.push(...errors.map(message => `${scenario}: page error: ${message}`));
  }
  await browser.close();
  fs.writeFileSync(path.join(output, 'measurements.json'), JSON.stringify(rows, null, 2) + '\n');
  console.log('| Scenario | Width | Words | Sizes | Height | Heading to button | Overflow |\n| --- | ---: | ---: | ---: | ---: | ---: | --- |');
  for (const row of rows) console.log(`| ${row.scenario} | ${row.width} | ${row.words} | ${row.sizes} | ${row.height} | ${row.headingToButton ?? '-'} | ${row.overflow ? 'yes' : 'no'} |`);
  console.log(`\nScreenshots: ${output}`);
  if (failures.length) throw new Error(failures.join('\n'));
})().catch(error => { console.error(error.message ?? error); process.exitCode = 1; });
