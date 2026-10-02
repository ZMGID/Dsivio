import { open } from '@tauri-apps/plugin-dialog'
import { Button } from '../../../../components/Button'
import { Field, StudioSelect } from '../../image/projects/StudioPanels'
import { RequirementComposer } from '../../image/projects/RequirementComposer'
import { EDIT_ASPECTS, EDIT_FITS, EDIT_RESOLUTIONS, parseEditPlan, serializeEditPlan, type EditPlan } from '../localEditContract'
import type { VideoFormProps } from './VideoFormFields'

function fileName(path: string) {
  return path.split(/[\\/]/).pop() || path
}

export function VideoEditForm(props: VideoFormProps) {
  const { brief, native, busy, dropActive, dropTarget, change, keepOsDrop, guarded, run, inputIssue } = props
  const clips = brief.clips || []
  const move = (index: number, delta: number) => {
    const next = clips.slice()
    const target = index + delta
    if (target < 0 || target >= next.length) return
    const [item] = next.splice(index, 1)
    next.splice(target, 0, item)
    change({ clips: next })
  }
  return (
    <div className="vs-layout">
      <div className="vs-editor-column">
        <section
          className={`vs-panel vs-drop${clips.length ? ' is-upload-area--filled' : ''}${dropActive && dropTarget !== 'music' && dropTarget !== 'subtitles' ? ' is-drop-active' : ''}`}
          data-video-drop="clips"
          aria-label="视频片段投放区"
          onDragEnter={event => keepOsDrop(event, 'clips')}
          onDragOver={event => keepOsDrop(event, 'clips')}
          onDrop={event => keepOsDrop(event, 'clips')}
        >
          <h3>视频片段</h3>
          <p className="vs-muted">按列表顺序拼接。可一次选择多段，或拖入 MP4 / MOV / WebM。</p>
          {clips.length > 0 && (
            <ol>
              {clips.map((path, index) => (
                <li key={path}>
                  <span title={path}>{fileName(path)}</span>
                  <Button size="sm" disabled={index === 0} aria-label={`上移 ${fileName(path)}`} onClick={() => move(index, -1)}>上移</Button>
                  <Button size="sm" disabled={index === clips.length - 1} aria-label={`下移 ${fileName(path)}`} onClick={() => move(index, 1)}>下移</Button>
                  <Button size="sm" aria-label={`移除 ${fileName(path)}`} onClick={() => change({ clips: clips.filter(item => item !== path) })}>移除</Button>
                </li>
              ))}
            </ol>
          )}
          <Button
            size="sm"
            disabled={!native || !!busy}
            onClick={() => void guarded('选择视频片段…', async current => {
              const result = await open({ multiple: true, filters: [{ name: '视频', extensions: ['mp4', 'mov', 'webm', 'mkv', 'm4v'] }] })
              if (!result || !current()) return
              const picked = Array.isArray(result) ? result : [result]
              change({ clips: [...new Set([...clips, ...picked])].slice(0, 12) })
            })}
          >
            添加视频片段
          </Button>
        </section>
        <section
          className="vs-panel"
          data-video-drop="music"
          aria-label="配乐投放区"
          onDragEnter={event => keepOsDrop(event, 'music')}
          onDragOver={event => keepOsDrop(event, 'music')}
          onDrop={event => keepOsDrop(event, 'music')}
        >
          <h3>配乐</h3>
          <p className="vs-muted" title={brief.musicPath}>{brief.musicPath ? fileName(brief.musicPath) : '可选。方案里可以调整音量。'}</p>
          <div className="vs-actions">
            <Button size="sm" disabled={!native || !!busy} onClick={() => void guarded('选择配乐…', async current => {
              const result = await open({ filters: [{ name: '音频', extensions: ['mp3', 'wav', 'm4a', 'aac'] }] })
              if (typeof result === 'string' && current()) change({ musicPath: result })
            })}>选择配乐</Button>
            {brief.musicPath && <Button size="sm" onClick={() => change({ musicPath: '' })}>移除配乐</Button>}
          </div>
        </section>
        <section
          className="vs-panel"
          data-video-drop="subtitles"
          aria-label="字幕投放区"
          onDragEnter={event => keepOsDrop(event, 'subtitles')}
          onDragOver={event => keepOsDrop(event, 'subtitles')}
          onDrop={event => keepOsDrop(event, 'subtitles')}
        >
          <h3>字幕</h3>
          <p className="vs-muted" title={brief.subtitlePath}>{brief.subtitlePath ? fileName(brief.subtitlePath) : '可选 .srt，确认后烧录进成片。'}</p>
          <div className="vs-actions">
            <Button size="sm" disabled={!native || !!busy} onClick={() => void guarded('选择字幕…', async current => {
              const result = await open({ filters: [{ name: '字幕', extensions: ['srt'] }] })
              if (typeof result === 'string' && current()) change({ subtitlePath: result })
            })}>选择字幕</Button>
            {brief.subtitlePath && <Button size="sm" onClick={() => change({ subtitlePath: '' })}>移除字幕</Button>}
          </div>
        </section>
        <RequirementComposer
          label="剪辑要求"
          value={brief.request}
          onChange={request => change({ request })}
          placeholder="例如：前两段各留开头 3 秒，竖屏，配乐放低，保留原声。"
        />
      </div>
      <section className="vs-panel vs-options">
        <Field label="画幅">
          <StudioSelect value={brief.ratio} onChange={event => change({ ratio: event.target.value })}>
            {EDIT_ASPECTS.map(value => <option key={value} value={value}>{value}</option>)}
          </StudioSelect>
        </Field>
        <Field label="适配">
          <StudioSelect value={brief.editFit || 'pad'} onChange={event => change({ editFit: event.target.value as 'pad' | 'crop' })}>
            {EDIT_FITS.map(value => <option key={value} value={value}>{value === 'pad' ? '留边 pad' : '裁切 crop'}</option>)}
          </StudioSelect>
        </Field>
        <Field label="清晰度">
          <StudioSelect value={brief.editResolution || ''} onChange={event => change({ editResolution: event.target.value as '' | '720p' | '1080p' })}>
            <option value="">保持素材</option>
            {EDIT_RESOLUTIONS.map(value => <option key={value} value={value}>{value}</option>)}
          </StudioSelect>
        </Field>
        {inputIssue && <p className="vs-muted" role="status">{inputIssue}</p>}
        <div className="vs-actions studio-primary-actions">
          <Button
            variant="primary"
            disabled={!native || !!busy || clips.length === 0 || !brief.request.trim()}
            onClick={() => void run('plan')}
          >
            生成剪辑方案
          </Button>
        </div>
      </section>
    </div>
  )
}

export function EditPlanEditor({ script, onChange }: { script: string; onChange: (script: string) => void }) {
  const plan = parseEditPlan(script)
  if (!plan) return null
  const update = (next: EditPlan) => onChange(serializeEditPlan(next))
  const patchClip = (index: number, patch: Partial<EditPlan['clips'][number]>) => {
    update({ ...plan, clips: plan.clips.map((clip, clipIndex) => clipIndex === index ? { ...clip, ...patch } : clip) })
  }
  return (
    <div className="vs-edit-plan">
      <ol>
        {plan.clips.map((clip, index) => {
          const name = fileName(clip.source)
          return (
            <li key={`${clip.source}:${index}`}>
              <strong title={clip.source}>{name}</strong>
              <label>
                入点
                <input className="kv-input" aria-label={`${name} 入点`} type="number" min={0} step="0.1" value={clip.start ?? ''} onChange={event => patchClip(index, { start: event.target.value === '' ? undefined : Number(event.target.value) })} />
              </label>
              <label>
                出点
                <input className="kv-input" aria-label={`${name} 出点`} type="number" min={0} step="0.1" value={clip.end ?? ''} onChange={event => patchClip(index, { end: event.target.value === '' ? undefined : Number(event.target.value) })} />
              </label>
              <Button size="sm" disabled={index === 0} aria-label={`方案上移 ${name}`} onClick={() => {
                const clips = plan.clips.slice()
                const [item] = clips.splice(index, 1)
                clips.splice(index - 1, 0, item)
                update({ ...plan, clips })
              }}>上移</Button>
              <Button size="sm" disabled={index === plan.clips.length - 1} aria-label={`方案下移 ${name}`} onClick={() => {
                const clips = plan.clips.slice()
                const [item] = clips.splice(index, 1)
                clips.splice(index + 1, 0, item)
                update({ ...plan, clips })
              }}>下移</Button>
            </li>
          )
        })}
      </ol>
      <Field label="方案画幅">
        <StudioSelect value={plan.aspect || ''} onChange={event => update({ ...plan, aspect: (event.target.value || undefined) as EditPlan['aspect'] })}>
          <option value="">不改变</option>
          {EDIT_ASPECTS.map(value => <option key={value} value={value}>{value}</option>)}
        </StudioSelect>
      </Field>
      <Field label="方案适配">
        <StudioSelect value={plan.fit || 'pad'} onChange={event => update({ ...plan, fit: event.target.value as EditPlan['fit'] })}>
          {EDIT_FITS.map(value => <option key={value} value={value}>{value}</option>)}
        </StudioSelect>
      </Field>
      <Field label="方案清晰度">
        <StudioSelect value={plan.resolution || ''} onChange={event => update({ ...plan, resolution: (event.target.value || undefined) as EditPlan['resolution'] })}>
          <option value="">保持素材</option>
          {EDIT_RESOLUTIONS.map(value => <option key={value} value={value}>{value}</option>)}
        </StudioSelect>
      </Field>
      <Field label="配乐路径">
        <input className="kv-input" aria-label="配乐路径" value={plan.music?.path || ''} onChange={event => update({ ...plan, music: event.target.value ? { ...plan.music, path: event.target.value, volume: plan.music?.volume, originalVolume: plan.music?.originalVolume } : undefined })} />
      </Field>
      <Field label="配乐音量">
        <input className="kv-input" aria-label="配乐音量" type="number" min={0} max={2} step="0.1" value={plan.music?.volume ?? ''} disabled={!plan.music?.path} onChange={event => update({ ...plan, music: plan.music ? { ...plan.music, volume: event.target.value === '' ? undefined : Number(event.target.value) } : undefined })} />
      </Field>
      <Field label="原声音量">
        <input className="kv-input" aria-label="原声音量" type="number" min={0} max={2} step="0.1" value={plan.music?.originalVolume ?? ''} disabled={!plan.music?.path} onChange={event => update({ ...plan, music: plan.music ? { ...plan.music, originalVolume: event.target.value === '' ? undefined : Number(event.target.value) } : undefined })} />
      </Field>
      <Field label="字幕路径">
        <input className="kv-input" aria-label="字幕路径" value={plan.subtitles?.path || ''} onChange={event => update({ ...plan, subtitles: event.target.value ? { path: event.target.value } : undefined })} />
      </Field>
    </div>
  )
}
