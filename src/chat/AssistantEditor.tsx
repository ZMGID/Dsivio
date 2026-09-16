import { useEffect, useRef, useState } from 'react'
import { Button } from '../components/Button'
import { Select } from '../settings/components'
import { chatApi } from './api'
import {
  ASSISTANT_PROMPT_CATEGORY_LABELS,
  ASSISTANT_PROMPT_CATEGORIES,
  assistantPromptCategory,
  type AssistantPromptCategory,
} from './assistantCategories'
import type { ChatAssistant } from './types'

export function AssistantEditor({ assistant, onSaved, onCancel }: {
  assistant?: ChatAssistant | null
  onSaved: (assistant: ChatAssistant) => void
  onCancel: () => void
}) {
  const [name, setName] = useState(assistant?.name ?? '')
  const [category, setCategory] = useState<AssistantPromptCategory>(
    assistant ? assistantPromptCategory(assistant) : 'general',
  )
  const [description, setDescription] = useState(assistant?.description ?? '')
  const [prompt, setPrompt] = useState(assistant?.system_prompt ?? assistant?.systemPrompt ?? '')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const save = async () => {
    if (busy || !name.trim() || !prompt.trim()) return
    setBusy(true)
    setError('')
    const now = Math.floor(Date.now() / 1000)
    const payload: ChatAssistant = {
      id: assistant?.id ?? `asst_${crypto.randomUUID()}`, name: name.trim(),
      system_prompt: prompt.trim(), description: description.trim(), icon: 'bot', color: '#6A8FBD',
      source: assistant?.source ?? 'user', category, provider_id: '', model: '', mcp_server_ids: [], skill_ids: [],
      enabled: true, installed: assistant?.installed ?? false, archived: false, built_in: false,
      created_at: assistant?.created_at ?? now, updated_at: now,
    }
    try {
      onSaved(await (assistant ? chatApi.updateAssistant(payload) : chatApi.createAssistant(payload)))
    } catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { setBusy(false) }
  }
  return <form className="space-y-5" onSubmit={e => { e.preventDefault(); void save() }}>
    <label className="block space-y-2 text-sm"><span>名称</span>
      <input autoFocus className="kv-input w-full" aria-label="助手名称" placeholder="例如：极简商品摄影" maxLength={64}
        disabled={busy} value={name} onChange={e => setName(e.target.value)} />
    </label>
    <div className="block space-y-2 text-sm"><span>分类</span>
      <Select className="w-full" ariaLabel="助手分类" disabled={busy} value={category}
        onChange={value => setCategory(value as AssistantPromptCategory)}
        options={ASSISTANT_PROMPT_CATEGORIES.map(value => ({
          value,
          label: ASSISTANT_PROMPT_CATEGORY_LABELS[value],
        }))} />
    </div>
    <label className="block space-y-2 text-sm"><span>描述</span>
      <textarea className="kv-textarea custom-scrollbar min-h-[76px] w-full resize-y" aria-label="助手描述" rows={3}
        placeholder="简单说明这个助手适合做什么，例如：把商品信息整理成 15 秒 UGC 带货视频脚本。"
        maxLength={240} disabled={busy} value={description} onChange={e => setDescription(e.target.value)} />
    </label>
    <label className="block space-y-2 text-sm"><span>系统提示词</span>
      <textarea className="kv-textarea custom-scrollbar w-full min-h-[280px]" aria-label="系统提示词" rows={12}
        placeholder="把你精选的专业提示词粘贴在这里。说明助手的角色、处理方法、风格和输出要求。"
        disabled={busy} value={prompt} onChange={e => setPrompt(e.target.value)} />
    </label>
    <p className="text-xs leading-relaxed text-neutral-500">图片和视频页面只显示对应分类与通用助手；聊天中可以使用全部助手。</p>
    {error && <p role="alert" className="text-sm text-red-500">{error}</p>}
    <div className="flex justify-end gap-2">
      <Button variant="ghost" disabled={busy} onClick={onCancel}>取消</Button>
      <Button type="submit" disabled={busy || !name.trim() || !prompt.trim()}>{busy ? '保存中…' : '保存助手'}</Button>
    </div>
  </form>
}

export function AssistantDialog({ assistant, onSaved, onClose }: {
  assistant?: ChatAssistant | null
  onSaved: (assistant: ChatAssistant) => void
  onClose: () => void
}) {
  const ref = useRef<HTMLDialogElement>(null)
  useEffect(() => { ref.current?.showModal() }, [])
  return <dialog ref={ref} className="assistant-prompt-dialog" onCancel={e => { e.preventDefault(); onClose() }}>
    <h2 className="mb-5 text-lg font-semibold">{assistant ? '编辑助手' : '新建助手'}</h2>
    <AssistantEditor assistant={assistant} onSaved={onSaved} onCancel={onClose} />
  </dialog>
}
