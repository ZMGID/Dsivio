import { useT } from '../../../components/i18n'
import { useRoles } from './useRoles'

// Video pages import the hook from this module alongside the picker.
// eslint-disable-next-line react-refresh/only-export-components
export { useRoles } from './useRoles'

/** Absolute reference-image paths for the selected role records. */
// Video pages import this helper from the same module as the picker.
// eslint-disable-next-line react-refresh/only-export-components
export function rolesToReferenceImages(roles: Array<{ images: string[] }>): string[] {
  const paths: string[] = []
  for (const role of roles) {
    for (const image of role.images) {
      if (image && !paths.includes(image)) paths.push(image)
    }
  }
  return paths
}

/**
 * Multi-select of saved roles. Video pages pass the chosen ids back
 * and resolve files with `rolesToReferenceImages`.
 */
export function RolePicker({
  value,
  onChange,
  max,
}: {
  value: string[]
  onChange: (ids: string[]) => void
  max?: number
}) {
  const t = useT()
  const { roles, error } = useRoles()
  const atMax = max != null && value.length >= max

  function toggle(id: string) {
    if (value.includes(id)) {
      onChange(value.filter((item) => item !== id))
      return
    }
    if (atMax) return
    onChange([...value, id])
  }

  return (
    <div className="workbench-chip-row" role="group" aria-label={t.workbenchNavRoles}>
      {error ? <p className="workbench-inline-note">{error}</p> : null}
      {roles.length === 0 ? <p className="workbench-page-sub">{t.workbenchRolesPickerEmpty}</p> : null}
      {roles.map((role) => {
        const selected = value.includes(role.id)
        return (
          <button
            key={role.id}
            type="button"
            className={`workbench-chip${selected ? ' is-active' : ''}`}
            aria-pressed={selected}
            disabled={!selected && atMax}
            onClick={() => toggle(role.id)}
          >
            {role.name}
          </button>
        )
      })}
    </div>
  )
}
