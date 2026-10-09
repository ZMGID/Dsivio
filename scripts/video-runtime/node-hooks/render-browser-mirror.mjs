// Loaded with `--import` through NODE_OPTIONS by `dsivio dsvideo` (src-tauri/src/media_runtime/dsvideo.rs),
// so every Node process of the bundled Dsvideo — including the HyperFrames Provider's browser
// preparation child — downloads Chrome for Testing archives from npmmirror first and falls back
// to the official source. Resolution of other modules is untouched.
import { register } from 'node:module'

register(new URL('./render-browser-mirror-hooks.mjs', import.meta.url))
