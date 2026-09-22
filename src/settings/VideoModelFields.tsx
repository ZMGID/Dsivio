import { useState } from 'react'
import { api } from '../api/tauri'
import { VIDEO_PROTOCOLS, VIDEO_CATALOG_VERIFIED_AT, videoModel, type VideoProtocol } from '../data/videoModels'
import { Button } from '../components/Button'
import { Select } from './components'

export function VideoModelFields({ model, protocol, baseUrl, lang, onChange }: {
  model: string
  protocol?: VideoProtocol
  baseUrl?: string
  lang: 'zh' | 'en'
  onChange: (protocol: VideoProtocol | undefined) => void
}) {
  const [preview, setPreview] = useState('')
  const [error, setError] = useState('')
  const [pending, setPending] = useState(false)
  const profile = videoModel(model)
  const definition = protocol ? VIDEO_PROTOCOLS[protocol] : undefined
  const zh = lang === 'zh'
  return <div className="kv-drawer-section min-w-0">
    <label className="kv-drawer-label">{zh ? '视频接口协议' : 'Video API protocol'}</label>
    <Select
      ariaLabel={zh ? '视频接口协议' : 'Video API protocol'}
      value={protocol ?? ''}
      onChange={value => { onChange((value || undefined) as VideoProtocol | undefined); setPreview(''); setError('') }}
      options={[{ value: '', label: zh ? '请选择视频协议' : 'Choose a video protocol' }, ...Object.entries(VIDEO_PROTOCOLS).map(([value, item]) => ({ value, label: item.label }))]}
    />
    <p className="kv-row-desc">{zh ? '独立于供应商的聊天协议。自定义模型 ID 必须明确指定，不根据名称猜测。' : 'Independent of the chat protocol. Custom model IDs require an explicit video protocol.'}</p>
    {definition && <>
      <p className="kv-row-desc break-words">{definition.notes}</p>
      <p className="kv-row-desc break-words">POST {(profile && profile.protocol === protocol && profile.createPath) || definition.createPath}<br />GET {definition.queryPath}</p>
      {profile && profile.protocol === protocol && <>
        <p className="kv-row-desc break-words">{[
          profile.durations.length ? `${zh ? '时长（秒）' : 'Seconds'}: ${profile.durations.join(', ')}` : '',
          profile.resolutions.length ? `${zh ? '分辨率' : 'Resolution'}: ${profile.resolutions.join(', ')}` : '',
          profile.ratios.length ? `${zh ? '比例' : 'Ratio'}: ${profile.ratios.join(', ')}` : '',
        ].filter(Boolean).join(' · ')}</p>
        {'notes' in profile && <p className="kv-row-desc">{profile.notes}</p>}
      </>}
      <div className="flex flex-wrap gap-2">
        <Button size="sm" variant="ghost" onClick={() => void api.openExternal(definition.sources[0])}>{zh ? '官方接口文档' : 'Official API docs'}</Button>
        <Button size="sm" disabled={pending} onClick={async () => {
          setPending(true); setError(''); setPreview('')
          try {
            const result = await api.previewVideoModelRequest({ model, protocol: protocol!, baseUrl: baseUrl || definition.baseUrl })
            setPreview(JSON.stringify(result, null, 2))
          } catch (failure) { setError(String(failure)) }
          finally { setPending(false) }
        }}>{pending ? (zh ? '检查中…' : 'Checking…') : (zh ? '预览请求（不提交）' : 'Preview request (no submission)')}</Button>
      </div>
      <p className="kv-row-desc">{zh ? '文档核对日期' : 'Docs checked'}: {VIDEO_CATALOG_VERIFIED_AT}</p>
      {error && <p role="alert" className="kv-row-desc">{error}</p>}
      {preview && <pre className="kv-row-desc whitespace-pre-wrap break-words">{preview}</pre>}
    </>}
  </div>
}
