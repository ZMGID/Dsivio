import { useT } from '../../../../components/i18n'
import { RolePicker, rolesToReferenceImages, useRoles } from '../../content/RolePicker'
import { DRAMA_STYLES, type DramaStyleId } from '../videoCatalog'
import { VideoBriefLayout, VideoProductFields, type VideoFormProps } from './VideoFormFields'

export function VideoDramaForm(props: VideoFormProps) {
  const t = useT()
  const { roles } = useRoles()
  const style = (props.brief.dramaStyle || 'twist') as DramaStyleId
  return (
    <VideoBriefLayout
      {...props}
      media={<VideoProductFields {...props} />}
      extraMedia={(
        <>
          <section className="vs-panel">
            <h3>短剧风格</h3>
            <div className="workbench-chip-row">
              {DRAMA_STYLES.map(item => (
                <button key={item.id} type="button" className={`workbench-chip${style === item.id ? ' is-active' : ''}`} aria-pressed={style === item.id} onClick={() => props.change({ dramaStyle: item.id })}>
                  {t[item.label]}
                </button>
              ))}
            </div>
          </section>
          <section className="vs-panel">
            <h3>出演角色</h3>
            <RolePicker
              max={6}
              value={props.brief.roleIds || []}
              onChange={ids => {
                const next = roles.filter(role => ids.includes(role.id))
                props.change({ roleIds: ids, roleImages: rolesToReferenceImages(next) })
              }}
            />
          </section>
        </>
      )}
    />
  )
}
