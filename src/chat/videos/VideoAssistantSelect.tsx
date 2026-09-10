import { useEffect, useState } from 'react'
import { api } from '../../api/tauri'
import { chatApi } from '../api'
import type { ChatAssistant } from '../types'
import { Field, StudioSelect } from '../images/StudioPanels'

const DEFAULT_ID = 'asst_builtin_video_prompt'
const ORDER = [DEFAULT_ID, 'asst_builtin_video_product', 'asst_builtin_video_ugc']

export function VideoAssistantSelect({ value, disabled, onChange }: {
  value?: string
  disabled: boolean
  onChange: (id: string) => void
}) {
  const [assistants, setAssistants] = useState<ChatAssistant[]>([])
  const [error, setError] = useState('')
  const [loaded, setLoaded] = useState(false)
  useEffect(() => {
    let alive = true
    const refresh = () => void chatApi.getAssistants().then(all => {
      if (!alive) return
      setAssistants(all.filter(a => a.category === 'video' && !a.archived).sort((a, b) => {
        const rank = (id: string) => ORDER.includes(id) ? ORDER.indexOf(id) : ORDER.length
        return rank(a.id) - rank(b.id) || a.name.localeCompare(b.name)
      }))
      setError('')
      setLoaded(true)
    }).catch(() => { if (alive) { setError('助手列表读取失败，请重新进入页面重试。'); setLoaded(true) } })
    refresh()
    const pending = api.onChatAssistantsChanged(refresh)
    return () => { alive = false; void pending.then(unlisten => unlisten()) }
  }, [])
  const selected = assistants.find(a => a.id === (value || DEFAULT_ID))
  const missing = loaded && !!value && value !== DEFAULT_ID && !selected
  return <Field label="提示词助手" hint={error || (missing ? '原助手不可用，请重新选择。' : selected?.description || '使用通用视频助手，按你的要求编写提示词。')}>
    <StudioSelect value={value === DEFAULT_ID ? '' : value || ''} disabled={disabled} onChange={e => onChange(e.target.value)}>
      <option value="">{assistants.find(a => a.id === DEFAULT_ID)?.name || '通用视频'}（默认）</option>
      {assistants.filter(a => a.id !== DEFAULT_ID).map(a => <option key={a.id} value={a.id}>{a.name}</option>)}
      {!!value && value !== DEFAULT_ID && !selected && <option value={value}>{loaded ? '原助手不可用' : '正在读取助手…'}</option>}
    </StudioSelect>
  </Field>
}
