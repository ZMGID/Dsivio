import { StudioSelect } from './StudioPanels'
import {
  imageOutputRatios,
  listImageOutputs,
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
  allowAuto = false,
  ratio,
  resolution,
  model,
  protocol,
  disabled,
  ariaLabel = '比例',
  onChange,
}: {
  allowAuto?: boolean
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
        onChange={(event) => onChange(allowAuto && (event.target.value === 'auto' || resolution === 'auto') ? { ratio: event.target.value, resolution } : snapImageOutput(model, protocol, event.target.value, resolution))}
      >
        {allowAuto && <option value="auto">Auto</option>}
        {ratios.filter(value => value !== 'auto').map((value) => (
          <option key={value} value={value}>{imageRatioLabel(value)}</option>
        ))}
      </StudioSelect>
    </div>
  )
}

export function ImageResolutionSelect({
  allowAuto = false,
  ratio,
  resolution,
  model,
  protocol,
  disabled,
  ariaLabel = '分辨率',
  onChange,
}: {
  allowAuto?: boolean
  ratio: string
  resolution: string
  model?: string
  protocol?: string
  disabled?: boolean
  ariaLabel?: string
  onChange: (next: { ratio: string; resolution: string }) => void
}) {
  const resolutions = ratio === 'auto' && allowAuto
    ? listImageOutputs(model, protocol).filter((item, index, all) => all.findIndex(other => other.resolution === item.resolution) === index)
    : imageOutputResolutions(model, protocol, ratio, resolution)
  const value = resolution
  return (
    <StudioSelect
      ariaLabel={ariaLabel}
      disabled={disabled}
      value={value}
      onChange={(event) => onChange({ ratio, resolution: event.target.value })}
    >
      {allowAuto && <option value="auto">Auto</option>}
      {resolutions.filter(item => item.resolution !== 'auto').map((item) => (
        <option key={item.resolution} value={item.resolution}>{ratio === 'auto' ? item.resolution.toUpperCase() : imageResolutionLabel(item)}</option>
      ))}
    </StudioSelect>
  )
}
