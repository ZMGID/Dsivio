# Models and Dsivio execution

Image and video generation use `@dsvideo/provider-dsivio` through `dsivio media`.
Do not configure vendor URLs, keys, OAuth or a fallback cloud Provider in Dsvideo.
The running Dsivio App owns credentials, enabled models and paid task state.

Inspect `dsivio media models` and run `dsvideo doctor --endpoint dsivio.media`.
Choose an authored model the adapter actually offers. A service model is not automatically
an authored Dsvideo model. Unsupported models and ports are reported rather than silently translated.

Current mapping covers GPT Image 2, Seedance 2/2 Fast/2.5, MiniMax H3 and Grok Imagine Video/1.5.
WhisperX alignment and system TTS are also served through the App (`dsivio media transcribe`,
`local/system-tts`). Rendering and media processing retain their local Providers.

## Work within the budget the user settled

Dsivio models may report no price (`Pricing unknown`). That means the rate is unpublished, not that
every extra request needs new approval. Agree the commission's scope and a rough budget once, then
use it without asking again for each command. Generated stills, scene or character images, keyframes
and a few short test clips are ordinary production steps inside that scope: plan them when they
make the piece better, rather than dropping them to save requests. For product videos, a generated
scene image that contains the product, passed together with the original product image as video
references, usually holds product fidelity and setting far better than prose alone.
Ask again only when the work clearly grows beyond what was agreed, such as many more clips, longer
or higher-resolution output, or repeated full regenerations.

A Profile selects `"dsivio.media": { "use": "@dsvideo/provider-dsivio" }` with no key or URL.
Optional `imageModel`/`videoModel` pins use exact App model IDs. An invalid pin is an error.
After changing App model choices, restart the Runtime when current work permits.

Never retry an uncertain submission. Inspect the original task receipt with `dsivio media status <id>`.
When the App is closed, open it to continue querying; do not switch to a direct vendor call.
