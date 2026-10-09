import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { after, test } from 'node:test'
import { fileURLToPath } from 'node:url'

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const runtime = resolve(process.env.DSIVIO_TEST_RUNTIME || join(repo, 'src-tauri/resources/video-runtime'))
const scratch = mkdtempSync(join(tmpdir(), 'dsivio-npm-regression-'))
const win = process.platform === 'win32'
const suffix = win ? '.exe' : ''
const node = join(runtime, win ? 'node/node.exe' : 'node/bin/node')
const npm = join(runtime, win ? 'node/node_modules/npm/bin/npm-cli.js' : 'node/lib/node_modules/npm/bin/npm-cli.js')
assert.ok(existsSync(node) && existsSync(npm), 'Run build:video-runtime first')
after(() => rmSync(scratch, { recursive: true, force: true }))

// Replace only resource discovery and home lookup; execute the real launch::run and npm.
const harness = join(scratch, 'launch-harness.rs')
writeFileSync(harness, `
extern crate self as directories;
pub struct BaseDirs(std::path::PathBuf);
impl BaseDirs {
    pub fn new() -> Option<Self> { Some(Self(std::env::var_os("DSIVIO_TEST_HOME")?.into())) }
    pub fn home_dir(&self) -> &std::path::Path { &self.0 }
}
mod utils { pub fn strip_windows_verbatim_prefix(path: std::path::PathBuf) -> std::path::PathBuf { path } }
mod runtime {
    pub fn tools_resource_directory() -> Result<std::path::PathBuf, String> {
        Ok(std::env::var_os("DSIVIO_TEST_RESOURCES").unwrap().into())
    }
    pub fn tools_at(root: &std::path::Path) -> Result<std::collections::BTreeMap<&'static str, std::path::PathBuf>, String> {
        Ok([("node", root.join(if cfg!(windows) { "node/node.exe" } else { "node/bin/node" })),
            ("npm", root.join(if cfg!(windows) { "node/node_modules/npm/bin/npm-cli.js" } else { "node/lib/node_modules/npm/bin/npm-cli.js" }))]
            .into_iter().filter(|(_, path)| path.is_file()).collect())
    }
}
#[path = ${JSON.stringify(join(repo, 'src-tauri/src/media_runtime/launch.rs'))}]
mod launch;
fn main() -> std::process::ExitCode { launch::run(launch::Tool::Npm, std::env::args_os().skip(1)) }
`)
const launcher = join(scratch, `dsivio-npm${suffix}`)
function compile(source, output) {
  const result = spawnSync('rustc', ['--edition=2021', '-A', 'dead_code', source, '-o', output], { encoding: 'utf8', timeout: 60000 })
  assert.equal(result.status, 0, result.stderr || String(result.error))
}
compile(harness, launcher)
const launchers = [['dsivio npm', launcher]]
const shimSource = join(repo, 'scripts/video-runtime/shim.rs')
if (existsSync(shimSource)) {
  const root = join(scratch, 'relocated app', 'video-runtime')
  const shim = join(root, 'shims', 'tools', `npm${suffix}`)
  mkdirSync(dirname(shim), { recursive: true })
  // Copy Node/npm to test the real relocatable executable without changing the App bundle.
  cpSync(join(runtime, 'node'), join(root, 'node'), { recursive: true })
  compile(shimSource, shim)
  launchers.push(['PATH npm', shim])
}
for (const [label, binary] of launchers) {
  function fixture() {
    const cwd = mkdtempSync(join(scratch, 'workspace-'))
    const home = join(cwd, 'private-home')
    mkdirSync(home)
    const userconfig = join(home, '.npmrc')
    writeFileSync(userconfig, '')
    writeFileSync(join(cwd, 'package.json'), '{"name":"dsivio-regression","version":"1.0.0","private":true}')
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^npm_config_/iu.test(key)))
    Object.assign(env, { DSIVIO_TEST_HOME: home, DSIVIO_TEST_RESOURCES: dirname(runtime),
      npm_config_userconfig: userconfig, npm_config_globalconfig: join(home, 'global.npmrc'),
      npm_config_cache: join(cwd, 'npm-cache'), npm_config_offline: 'true',
      npm_config_script_shell: win ? process.env.ComSpec : '/bin/sh' })
    return { cwd, home, userconfig, env }
  }
  function run(f, args) {
    const result = spawnSync(binary, args, { cwd: f.cwd, env: f.env, encoding: 'utf8', timeout: 20000 })
    assert.equal(result.error, undefined)
    return result
  }
  function execute(f, code, command = "exec") {
    const script = join(f.cwd, 'command.cjs')
    writeFileSync(script, code)
    // npm exec -c executes a local command and does not fetch any package.
    return run(f, [command, '-c', `"${node}" "${script}"`])
  }
  test(`${label}: failed local command runs exactly once`, () => {
    const f = fixture()
    const effect = join(f.cwd, 'effects.txt')
    const result = execute(f, `require('node:fs').appendFileSync(${JSON.stringify(effect)}, 'ran\\n'); process.exit(7)`)
    assert.equal(result.status, 7, result.stderr)
    assert.equal(readFileSync(effect, 'utf8'), 'ran\n')
    assert.doesNotMatch(result.stderr, /重试/u)
  })
  test(`${label}: npm x alias also runs a failed command once`, () => {
    const f = fixture()
    const effect = join(f.cwd, 'effects.txt')
    const result = execute(f, `require('node:fs').appendFileSync(${JSON.stringify(effect)}, 'ran\\n'); process.exit(9)`, 'x')
    assert.equal(result.status, 9, result.stderr)
    assert.equal(readFileSync(effect, 'utf8'), 'ran\n')
  })
  test(`${label}: explicit globalconfig registry and prefix are retained`, () => {
    const f = fixture()
    const prefix = join(f.cwd, 'global-config-prefix')
    writeFileSync(f.env.npm_config_globalconfig, `registry=https://global.example.invalid/\nprefix=${prefix}\n`)
    const result = execute(f, 'console.log(process.env.npm_config_registry)')
    assert.equal(result.status, 0, result.stderr)
    assert.equal(result.stdout.trim(), 'https://global.example.invalid/')
    assert.equal(run(f, ['prefix', '-g']).stdout.trim(), prefix)
  })
  test(`${label}: user npmrc registry is retained during exec`, () => {
    const f = fixture()
    writeFileSync(f.userconfig, 'registry=https://registry.example.invalid/\n')
    const result = execute(f, 'console.log(process.env.npm_config_registry)')
    assert.equal(result.status, 0, result.stderr)
    assert.equal(result.stdout.trim(), 'https://registry.example.invalid/')
  })
  test(`${label}: user npmrc prefix is retained`, () => {
    const f = fixture()
    const prefix = join(f.cwd, 'custom-global')
    writeFileSync(f.userconfig, `prefix=${prefix}\n`)
    const result = run(f, ['prefix', '-g'])
    assert.equal(result.status, 0, result.stderr)
    assert.equal(result.stdout.trim(), prefix)
  })
  test(`${label}: prefix environment and explicit argument are retained`, () => {
    const f = fixture()
    f.env.npm_config_prefix = join(f.cwd, 'environment-global')
    assert.equal(run(f, ['prefix', '-g']).stdout.trim(), f.env.npm_config_prefix)
    const prefix = join(f.cwd, 'argument-global')
    assert.equal(run(f, ['prefix', '-g', `--prefix=${prefix}`]).stdout.trim(), prefix)
  })
  test(`${label}: nested project npmrc is retained`, () => {
    const f = fixture()
    writeFileSync(join(f.cwd, '.npmrc'), 'registry=https://project.example.invalid/\n')
    f.cwd = join(f.cwd, 'nested')
    mkdirSync(f.cwd)
    const result = execute(f, 'console.log(process.env.npm_config_registry)')
    assert.equal(result.status, 0, result.stderr)
    assert.equal(result.stdout.trim(), 'https://project.example.invalid/')
  })
}
