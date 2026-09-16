import { useState } from 'react'
import { RequirementOptimize, type RequirementOptimizePurpose } from './RequirementOptimize'

export function RequirementComposer({
  label,
  value,
  onChange,
  onError,
  disabled,
  placeholder,
  preferredAssistantId,
  purpose,
  mediaPaths,
  onAssistantChange,
}: {
  label: string
  value: string
  onChange: (next: string) => void
  onError?: (message: string) => void
  onAssistantChange?: (id: string) => void
  disabled?: boolean
  placeholder: string
  preferredAssistantId?: string
  purpose?: RequirementOptimizePurpose
  mediaPaths?: Array<string | null | undefined>
}) {
  const [error, setError] = useState('')
  return (
    <div className="is-req">
      <div className="is-req-bar">
        <span>{label}</span>
        <RequirementOptimize
          value={value}
          disabled={disabled}
          preferredAssistantId={preferredAssistantId}
          purpose={purpose}
          mediaPaths={mediaPaths}
          onChange={onChange}
          onError={onError ?? setError}
          onAssistantChange={onAssistantChange}
        />
      </div>
      <textarea
        className="kv-textarea custom-scrollbar"
        aria-label={label}
        disabled={disabled}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
      />
      {error && <p role="alert" className="text-xs text-red-500">{error}</p>}
    </div>
  )
}
