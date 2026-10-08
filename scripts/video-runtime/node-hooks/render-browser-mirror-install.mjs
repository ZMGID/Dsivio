// Mirror-first wrapper around @puppeteer/browsers `install()` for Chrome for Testing archives
// (chrome, chrome-headless-shell, chromedriver). npmmirror serves the same
// `<base>/<version>/<platform>/<archive>.zip` layout as storage.googleapis.com.
// - An explicit `baseUrl` or custom `providers` (e.g. HyperFrames `browserDownloadBaseUrl`) is respected.
// - DSIVIO_RENDER_BROWSER_MIRROR=off disables the mirror; a URL selects another compatible mirror.
// - On any mirror failure the partial installation is removed and the official source is used once.
export const DEFAULT_MIRROR = 'https://cdn.npmmirror.com/binaries/chrome-for-testing'
const CHROME_FOR_TESTING = new Set(['chrome', 'chrome-headless-shell', 'chromedriver'])

export function mirrorBase(env = process.env) {
  const value = (env.DSIVIO_RENDER_BROWSER_MIRROR ?? '').trim()
  if (value === '') return DEFAULT_MIRROR
  if (['0', 'off', 'false', 'none', 'official'].includes(value.toLowerCase())) return undefined
  return value.replace(/\/+$/u, '')
}

export function wrapInstall(browsers, env = process.env, log = message => process.stderr.write(message)) {
  return async function install(options) {
    const mirror = mirrorBase(env)
    if (mirror === undefined || options === null || typeof options !== 'object' || options.baseUrl !== undefined
      || (options.providers?.length ?? 0) > 0 || !CHROME_FOR_TESTING.has(options.browser)) {
      return browsers.install(options)
    }
    log(`Dsivio：通过镜像 ${mirror} 下载 ${options.browser} ${options.buildId}（失败时自动改用官方源）\n`)
    try {
      return await browsers.install({ ...options, baseUrl: mirror })
    } catch (error) {
      log(`Dsivio：镜像下载失败（${error instanceof Error ? error.message : String(error)}），改用官方源重试一次…\n`)
      try {
        await browsers.uninstall({ browser: options.browser, buildId: options.buildId, cacheDir: options.cacheDir, ...(options.platform === undefined ? {} : { platform: options.platform }) })
      } catch { /* Nothing was installed. */ }
      return browsers.install(options)
    }
  }
}
