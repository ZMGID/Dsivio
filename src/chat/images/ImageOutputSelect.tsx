import { StudioSelect } from './StudioPanels'
import {
  imageOutputRatios,
  imageOutputResolutions,
  imageRatioLabel,
  imageResolutionLabel,
  snapImageOutput,
} from './imageOutput'

function frameBox(ratio: string, max = 16): { width: number; height: number } {
  const [width, height] = ratio.split(':').map(Number)
  const w = width || 1
  const h = height || 1
  const scale = max / Math.max(w, h)
  return {
    width: Math.max(7, Math.round(w * scale)),
    height: Math.max(7, Math.round(h * scale)),
  }
}

export function ImageRatioSelect({
  ratio,
  resolution,
  model,
  protocol,
  disabled,
  ariaLabel = '比例',
  onChange,
}: {
  ratio: string
  resolution: string
  model?: string
  protocol?: string
  disabled?: boolean
  ariaLabel?: string
  onChange: (next: { ratio: string; resolution: string }) => void
}) {
  const ratios = imageOutputRatios(model, protocol, ratio)
  return (
    <div className="if-output">
      <span className="if-output-frame" aria-hidden>
        <i style={frameBox(ratio)} />
      </span>
      <StudioSelect
        ariaLabel={ariaLabel}
        disabled={disabled}
        value={ratio}
        onChange={(event) => onChange(snapImageOutput(model, protocol, event.target.value, resolution))}
      >
        {ratios.map((value) => (
          <option key={value} value={value}>{imageRatioLabel(value)}</option>
        ))}
      </StudioSelect>
    </div>
  )
}

export function ImageResolutionSelect({
  ratio,
  resolution,
  model,
  protocol,
  disabled,
  ariaLabel = '分辨率',
  onChange,
}: {
  ratio: string
  resolution: string
  model?: string
  protocol?: string
  disabled?: boolean
  ariaLabel?: string
  onChange: (next: { ratio: string; resolution: string }) => void
}) {
  const resolutions = imageOutputResolutions(model, protocol, ratio, resolution)
  const value = resolution
  return (
    <StudioSelect
      ariaLabel={ariaLabel}
      disabled={disabled}
      value={value}
      onChange={(event) => onChange({ ratio, resolution: event.target.value })}
    >
      {resolutions.map((item) => (
        <option key={item.resolution} value={item.resolution}>{imageResolutionLabel(item)}</option>
      ))}
    </StudioSelect>
  )
}
