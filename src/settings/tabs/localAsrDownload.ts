/** Measured install sizes. The environment and recognition model are shared by all languages. */
const ENVIRONMENT_GB = 1.5
/** WhisperX aligns these with small torchaudio models; the rest use ~1.3 GB Hugging Face models. */
const TORCHAUDIO_LANGUAGES = new Set(['en', 'fr', 'de', 'es', 'it'])

export function estimateLocalAsrDownloadGb(languages: string[]): number {
  return [...new Set(languages)].reduce((total, language) => total + (TORCHAUDIO_LANGUAGES.has(language) ? 0.36 : 1.27), ENVIRONMENT_GB)
}
