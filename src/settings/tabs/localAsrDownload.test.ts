import { describe, expect, it } from 'vitest'
import { estimateLocalAsrDownloadGb } from './localAsrDownload'

describe('estimateLocalAsrDownloadGb', () => {
  it('counts the shared environment once plus each language alignment model', () => {
    // Environment and recognition model ~1.5 GB; English uses a small torchaudio model.
    expect(estimateLocalAsrDownloadGb(['en'])).toBeCloseTo(1.9, 1)
    // Portuguese and Chinese use ~1.3 GB Hugging Face models each.
    expect(estimateLocalAsrDownloadGb(['en', 'pt'])).toBeCloseTo(3.1, 1)
    expect(estimateLocalAsrDownloadGb(['zh', 'en', 'pt'])).toBeCloseTo(4.4, 1)
  })
})
