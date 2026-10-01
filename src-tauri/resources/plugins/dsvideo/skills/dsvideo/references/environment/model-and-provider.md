# Models and Dsivio execution

Image and video generation use `@dsvideo/provider-dsivio` through `dsivio media`.
Do not configure vendor URLs, keys, OAuth or a fallback cloud Provider in Dsvideo.
The running Dsivio App owns credentials, enabled models and paid task state.

Inspect `dsivio media models` and run `dsvideo doctor --endpoint dsivio.media`.
Choose an authored model the adapter actually offers. A service model is not automatically
an authored Dsvideo model. Unsupported models and ports are reported rather than silently translated.

Current mapping covers GPT Image 2, Seedance 2/2 Fast/2.5, MiniMax H3 and Grok Imagine Video/1.5.
Speech generation and cloud ASR are not offered by this adapter yet. Local WhisperX is available
through its retained local Provider. Rendering and media processing retain their local Providers.

A Profile selects `"dsivio.media": { "use": "@dsvideo/provider-dsivio" }` with no key or URL.
Optional `imageModel`/`videoModel` pins use exact App model IDs. An invalid pin is an error.
After changing App model choices, restart the Runtime when current work permits.

Never retry an uncertain submission. Inspect the original task receipt with `dsivio media status <id>`.
When the App is closed, open it to continue querying; do not switch to a direct vendor call.
