// Keep the standalone SKU plugin's report helpers identical to the existing report pipeline.
// Edit the Shopee report helpers, then run this script; --check rejects stale packaged copies.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const source = path.join(root, 'src-tauri/resources/plugins/shopee-research/skills/shopee-research/scripts')
const target = path.join(root, 'src-tauri/resources/plugins/product-sku-search/skills/product-sku-search/scripts')
const files = ['build_report.py', 'finalize_report.py', 'prepare_images.py', 'embed_images.py', 'setup_runtime.py', 'requirements.txt']
if (!process.argv.includes('--check')) mkdirSync(target, { recursive: true })
for (const file of files) {
  const content = readFileSync(path.join(source, file))
  if (process.argv.includes('--check')) {
    if (!readFileSync(path.join(target, file)).equals(content)) throw new Error(`Run node scripts/sync-product-sku-report.mjs: ${file}`)
  } else writeFileSync(path.join(target, file), content)
}
console.log(`SKU report helpers ${process.argv.includes('--check') ? 'checked' : 'synced'} (${files.length}).`)
