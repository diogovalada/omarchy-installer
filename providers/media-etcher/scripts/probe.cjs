// Read-only module loading probe. Does not enumerate devices or open media.
const fs = require('node:fs');
const path = require('node:path');
const report = {
  node: process.version,
  platform: process.platform,
  arch: process.arch,
  model: 'gpt-6-astra',
  sdk: require('etcher-sdk/package.json').version,
  modules: {},
};
for (const name of ['etcher-sdk', 'etcher-sdk/build/multi-write', 'drivelist', 'mountutils',
  '@ronomon/direct-io', 'lzma-native', 'xxhash-addon', '@balena/node-crc-utils']) {
  try { require(name); report.modules[name] = 'loaded'; }
  catch (error) { report.modules[name] = { error: error.message }; process.exitCode = 1; }
}
const natives = [];
function walk(dir) {
  for (const item of fs.readdirSync(dir, { withFileTypes: true })) {
    const filename = path.join(dir, item.name);
    if (item.isDirectory()) walk(filename);
    else if (item.isFile() && item.name.endsWith('.node')) natives.push(path.relative(path.join(__dirname, '..'), filename));
  }
}
walk(path.join(__dirname, '../node_modules'));
report.nativeBinaries = natives;
console.log(JSON.stringify(report, null, 2));
