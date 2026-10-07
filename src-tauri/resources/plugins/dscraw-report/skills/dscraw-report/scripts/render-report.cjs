const fs = require('fs');
const http = require('http');
const os = require('os');
const path = require('path');

function fail(message) {
  process.stderr.write(`${message}\n`);
  process.exit(1);
}

function loadPlaywright() {
  try {
    return require('playwright');
  } catch (_) {
    const bundled = path.join(
      os.homedir(),
      '.cache',
      'codex-runtimes',
      'codex-primary-runtime',
      'dependencies',
      'node',
      'node_modules',
      'playwright'
    );
    return require(bundled);
  }
}

function cents(value, label) {
  const normalized = String(value).replace(/\s/g, '').replace(',', '.');
  if (!/^\d+(?:\.\d{1,2})?$/.test(normalized)) fail(`${label} 不是有效金额: ${value}`);
  return Math.round(Number(normalized) * 100);
}

const inputPath = process.argv[2];
if (!inputPath) fail('用法: node render-report.cjs <report-data.json> [日报根目录]');

const data = JSON.parse(fs.readFileSync(path.resolve(inputPath), 'utf8'));
const snapshot = data.configSnapshot;
if (!snapshot || !Array.isArray(snapshot.stores)) fail('缺少配置快照，请先运行 report_state.py prepare');
if (!/^\d{4}-\d{2}-\d{2}$/.test(data.dateIso || '')) fail('dateIso 必须是 YYYY-MM-DD');
if (!Array.isArray(data.stores) || !data.stores.length || data.stores.length !== snapshot.stores.length) fail('结果店铺数与配置不一致');
const ids = new Set();
let capturedCount = 0;
const salesTotals = Object.create(null);
const quantityTotals = Object.create(null);
data.stores.forEach((store, index) => {
  const config = snapshot.stores[index];
  if (store.storeId !== config.storeId || ids.has(store.storeId)) fail('结果店铺身份或顺序与配置不一致');
  ids.add(store.storeId);
  for (const key of ['currency', 'orderMetric', 'collectAds']) {
    if (store[key] !== config[key]) fail(`店铺 ${key} 与配置不一致`);
  }
  if ((store.sales == null || store.orders == null) && !store.missingReason) fail('经营指标缺失时须说明原因');
  if (store.sales != null) salesTotals[store.currency] = (salesTotals[store.currency] || 0) + cents(store.sales, 'sales');
  if (store.orders != null) {
    if (!Number.isInteger(store.orders) || store.orders < 0) fail('orders 必须是非负整数');
    const quantity = quantityTotals[store.orderMetric] ||= { label: store.orderLabel, value: 0 };
    quantity.value += store.orders;
  }
  if (store.sales != null && store.orders != null) capturedCount++;
  if (store.collectAds) {
    if ((store.adCost == null || store.roi == null) && !store.adMissingReason && !store.missingReason) fail('广告指标缺失时须说明原因');
    if (store.adCost != null) cents(store.adCost, 'adCost');
    if (store.roi != null && !/^\d+(?:[.,]\d+)?$/.test(String(store.roi))) fail('roi 无效');
  }
  const fields = ['sales', 'orders', ...(store.collectAds ? ['adCost', 'roi'] : [])];
  const present = fields.filter(key => store[key] != null).length;
  store.status = present === fields.length ? '成功' : present === 0 ? '未获取' : '部分获取';
});
const report = {
  title: snapshot.report.title,
  dateIso: data.dateIso,
  dateText: data.dateText || data.dateIso,
  salesTotals: Object.fromEntries(Object.entries(salesTotals).map(([key, value]) => [key, (value / 100).toFixed(2)])),
  quantityTotals,
  capturedCount,
  expectedCount: snapshot.stores.length,
  stores: data.stores
};

const skillRoot = path.resolve(__dirname, '..');
const selectedOutput = process.argv[3] === '--output' ? JSON.parse(process.argv[4]) : null;
if (selectedOutput) snapshot.report.template = selectedOutput.template;
if (!snapshot.report.template || /[<>:\"/\\|?*]/.test(snapshot.report.template) || ['.', '..'].includes(snapshot.report.template)) fail('无效模板名称');
const templatePath = path.join(skillRoot, 'templates', `${snapshot.report.template}.html`);
if (!fs.existsSync(templatePath)) fail('所选日报模板不存在');
const outputRoot = path.resolve((!selectedOutput && process.argv[3]) || snapshot.report.outputRoot);
const outputDir = path.join(outputRoot, '日报', data.dateIso);
if (!selectedOutput) fs.mkdirSync(outputDir, { recursive: true });
if (!snapshot.report.filePrefix || /[<>:\"/\\|?*]/.test(snapshot.report.filePrefix)) fail('无效文件名前缀');
const baseName = `${snapshot.report.filePrefix}_${data.dateIso}`;
const htmlPath = selectedOutput ? selectedOutput.html : path.join(outputDir, `${baseName}.html`);
const pngPath = selectedOutput ? selectedOutput.png : path.join(outputDir, `${baseName}.png`);
fs.mkdirSync(path.dirname(htmlPath), { recursive: true });
const documentTitle = `${report.title} ${report.dateText} 日报`;
const reportJson = JSON.stringify(report).replace(/</g, '\\u003c');
const sourceLines = (data.sources || []).map((line) => `      ${String(typeof line === 'string' ? line : JSON.stringify(line)).replace(/\*\//g, '* /').replace(/</g, '&lt;')}`).join('\n');

const html = fs.readFileSync(templatePath, 'utf8')
  .replace('__DOCUMENT_TITLE__', documentTitle.replace(/[&<>]/g, c => ({'&':'&amp;', '<':'&lt;', '>':'&gt;'}[c])))
  .replace('__DATE_ISO__', data.dateIso)
  .replace('__REPORT_JSON__', reportJson)
  .replace('__SOURCES__', sourceLines);

if (/__[A-Z_]+__/.test(html)) fail('HTML 模板仍有未替换占位符');
fs.writeFileSync(htmlPath, html, 'utf8');

const server = http.createServer((request, response) => {
  response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
  response.end(html);
});

server.listen(0, '127.0.0.1', async () => {
  let browser;
  try {
    const { chromium } = loadPlaywright();
    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage({ viewport: { width: 1136, height: 645 }, deviceScaleFactor: 1 });
    await page.goto(`http://127.0.0.1:${server.address().port}/`, { waitUntil: 'networkidle' });
    const height = await page.evaluate(() => Math.max(645, document.documentElement.scrollHeight));
    await page.setViewportSize({ width: 1136, height });
    if (pngPath) await page.screenshot({ path: pngPath, fullPage: false });

    const checks = await page.evaluate(() => {
      const clipped = [...document.querySelectorAll('.top h1, .summary, .metric b, .store-name, .ads')]
        .filter((node) => node.scrollWidth > node.clientWidth)
        .map((node) => node.textContent.trim());
      return {
        title: document.title,
        heading: document.querySelector('h1')?.textContent.trim(),
        storeCount: document.querySelectorAll('.store-row').length,
        clipped,
        overflow: document.documentElement.scrollWidth > window.innerWidth ||
          document.documentElement.scrollHeight > window.innerHeight
      };
    });

    if (checks.storeCount !== report.expectedCount) fail(`渲染店铺数错误: ${checks.storeCount}`);
    if (checks.clipped.length) fail(`发现横向截断: ${checks.clipped.join(' | ')}`);
    if (checks.overflow) fail('页面超出动态画布');
    if (pngPath && (!fs.existsSync(pngPath) || fs.statSync(pngPath).size === 0)) fail('PNG 未生成');

    process.stdout.write(JSON.stringify({
      ok: true,
      htmlPath,
      pngPath,
      salesTotals: report.salesTotals,
      quantityTotals: report.quantityTotals,
      height,
      capturedCount,
      missingStores: data.stores.filter((store) => store.missingReason || store.adMissingReason).map((store) => store.name),
      checks
    }, null, 2));
  } catch (error) {
    fail(error.stack || error.message);
  } finally {
    if (browser) await browser.close();
    server.close();
  }
});
