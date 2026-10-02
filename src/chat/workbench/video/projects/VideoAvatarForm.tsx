import { RolePicker, rolesToReferenceImages, useRoles } from '../../content/RolePicker'
import { VideoBriefLayout, VideoProductFields, type VideoFormProps } from './VideoFormFields'

export function VideoAvatarForm(props: VideoFormProps) {
  const { roles } = useRoles()
  const selected = roles.filter(role => (props.brief.roleIds || []).includes(role.id))
  return (
    <VideoBriefLayout
      {...props}
      media={<VideoProductFields {...props} />}
      extraMedia={(
        <section className="vs-panel">
          <h3>出镜角色</h3>
          <RolePicker
            max={2}
            value={props.brief.roleIds || []}
            onChange={ids => {
              const next = roles.filter(role => ids.includes(role.id))
              props.change({ roleIds: ids, roleImages: rolesToReferenceImages(next) })
            }}
          />
          {selected.length > 0 && <p className="vs-muted">{selected.map(role => role.name).join('、')}</p>}
        </section>
      )}
    />
  )
}
