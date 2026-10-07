const path = require('path');
const fs = require('fs');
const os = require('os');

function loadPlaywright() {
  try { return require('playwright'); } catch (_) {}
const roots = [
    path.join(os.homedir(), '.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'),
    path.join(os.homedir(), '.codex/node_modules/playwright'),
    path.join(process.env.APPDATA || path.join(os.homedir(), 'AppData/Roaming'), 'npm/node_modules/playwright')
  ];
  for (const root of roots) {
    if (fs.existsSync(root)) return require(root);
  }
  throw new Error('playwright not found');
}

(async () => {
  const [htmlPath, pngPath] = process.argv.slice(2);
  if (!htmlPath || !pngPath) throw new Error('usage: screenshot_html.cjs input.html output.png');
  const { chromium } = loadPlaywright();
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 1 });
    await page.goto(`file:///${path.resolve(htmlPath).replace(/\\/g, '/')}`, { waitUntil: 'load' });
    await page.screenshot({ path: pngPath, fullPage: true });
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exit(1); });
