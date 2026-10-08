// Module customization hooks: `import ... from '@puppeteer/browsers'` receives the real module
// with `install` wrapped by render-browser-mirror-install.mjs. The wrapper is the real file URL
// plus a marker query, so other loaders (tsx) still see an ordinary file: parent URL.
const MARKER = 'dsivio-render-browser-mirror'
const PACKAGE = '@puppeteer/browsers'
const INSTALL = new URL('./render-browser-mirror-install.mjs', import.meta.url).href

export async function resolve(specifier, context, nextResolve) {
  if (specifier !== PACKAGE) return nextResolve(specifier, context)
  const resolved = await nextResolve(specifier, context)
  if (!resolved.url.startsWith('file:')) return resolved
  const url = new URL(resolved.url)
  url.searchParams.set(MARKER, '1')
  return { ...resolved, url: url.href, format: 'module', shortCircuit: true }
}

export async function load(url, context, nextLoad) {
  if (!url.startsWith('file:') || !new URL(url).searchParams.has(MARKER)) return nextLoad(url, context)
  const real = new URL(url)
  real.searchParams.delete(MARKER)
  const target = JSON.stringify(real.href)
  return {
    format: 'module',
    shortCircuit: true,
    source: `import * as browsers from ${target};\nimport { wrapInstall } from ${JSON.stringify(INSTALL)};\nexport * from ${target};\nexport const install = wrapInstall(browsers);\n`,
  }
}
