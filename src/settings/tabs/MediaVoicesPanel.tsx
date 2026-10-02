import { useEffect, useRef, useState } from 'react'
import { RefreshCw, Trash2 } from 'lucide-react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { api, type Settings, type VoiceReference } from '../../api/tauri'
import type { Lang } from '../../components/i18n'
import { IconButton } from '../../components/Button'
import { confirmDialog } from '../../components/dialogQueue'
import { SettingsGroup } from '../components'

const PROTOCOL_LABEL: Record<string, [string, string]> = {
  minimax_tts: ['MiniMax 语音', 'MiniMax speech'],
  openai_tts: ['OpenAI 语音', 'OpenAI speech'],
}

function expiryText(expiresAt: string, zh: boolean): { text: string; expired: boolean } {
  const days = Math.ceil((new Date(expiresAt).getTime() - Date.now()) / 86_400_000)
  if (days <= 0) return { text: zh ? '供应商侧已过期' : 'Expired at the provider', expired: true }
  return { text: zh ? `${days} 天后在供应商侧过期` : `Expires at the provider in ${days} day${days === 1 ? '' : 's'}`, expired: false }
}

export function MediaVoicesPanel({ settings, lang }: { settings: Pick<Settings, 'providers'>; lang: Lang }) {
  const zh = lang === 'zh'
  const [voices, setVoices] = useState<VoiceReference[] | null>(null)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [busyId, setBusyId] = useState<string | null>(null)
  const [reload, setReload] = useState(0)
  const mounted = useRef(false)
  // A refresh started before a delete must not bring the deleted reference back.
  const revision = useRef(0)
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  useEffect(() => {
    let alive = true
    const current = ++revision.current
    void api.listMediaVoices().then(result => { if (alive && revision.current === current) setVoices(result) })
      .catch(failure => { if (alive && revision.current === current) setError(String(failure)) })
    return () => { alive = false }
  }, [reload])

  async function remove(voice: VoiceReference) {
    const ok = await confirmDialog({
      message: zh
        ? `删除本地声音「${voice.voiceId}」？只删除本机记录，不会删除供应商那边的声音或已生成的音频。`
        : `Delete the local voice "${voice.voiceId}"? Only this device's record is removed; the provider voice and generated audio stay.`,
      confirmLabel: zh ? '删除' : 'Delete',
      danger: true,
    })
    if (!ok) return
    revision.current += 1
    setBusyId(voice.id); setError(''); setNotice('')
    try {
      await api.deleteMediaVoice(voice.id)
      if (mounted.current) {
        setVoices(current => current?.filter(item => item.id !== voice.id) ?? null)
        setNotice(zh ? `已删除「${voice.voiceId}」的本地记录。` : `Deleted the local record of "${voice.voiceId}".`)
      }
    } catch (failure) { if (mounted.current) setError(String(failure)) }
    finally { if (mounted.current) setBusyId(null) }
  }

  const providerName = (id: string) => settings.providers.find(provider => provider.id === id)?.name || id
  return <SettingsGroup title={<div className="kv-media-group-title">
    <span>{zh ? '已保存的声音' : 'Saved voices'} {voices && <span className="kv-media-count">{voices.length}</span>}</span>
    <IconButton size="xs" variant="ghost" label={zh ? '刷新声音列表' : 'Refresh voices'} onClick={() => { setError(''); setNotice(''); setReload(value => value + 1) }}>
      <RefreshCw />
    </IconButton>
  </div>}>
    {notice && <p className="kv-row-desc kv-media-empty" role="status">{notice}</p>}
    {error && <div className="kv-media-alert" role="alert"><span className="[overflow-wrap:anywhere]">{error}</span></div>}
    {voices === null && !error && <p className="kv-row-desc kv-media-empty">{zh ? '正在读取…' : 'Loading…'}</p>}
    {voices?.length === 0 && <p className="kv-row-desc kv-media-empty">{zh
      ? '还没有克隆的声音。用授权样本克隆后，声音会保存在这里供试听和复用。'
      : 'No cloned voices yet. Voices cloned from an authorized sample are kept here for playback and reuse.'}</p>}
    {voices && voices.length > 0 && <ul className="kv-media-list">{voices.map(voice => {
      const expiry = voice.expiresAt ? expiryText(voice.expiresAt, zh) : null
      return <li key={voice.id} className="kv-media-row kv-media-voice">
        <div className="min-w-0 flex-1">
          <div className="kv-media-row-name">
            <span className="kv-row-label font-mono [overflow-wrap:anywhere]">{voice.voiceId}</span>
            {voice.used ? <span className="kv-tag">{zh ? '已使用' : 'Used'}</span> : <span className="kv-tag accent">{zh ? '未使用' : 'Unused'}</span>}
            {expiry?.expired && <span className="kv-tag warn">{zh ? '已过期' : 'Expired'}</span>}
          </div>
          <p className="kv-row-desc">{providerName(voice.providerId)} · {PROTOCOL_LABEL[voice.protocol]?.[zh ? 0 : 1] ?? voice.protocol}
            {expiry && !expiry.expired && ` · ${expiry.text}`}</p>
          {voice.preview && <audio className="kv-media-audio" controls preload="metadata" src={convertFileSrc(voice.preview.path)} aria-label={`${zh ? '试听' : 'Play'} ${voice.voiceId}`} />}
        </div>
        <IconButton size="sm" variant="danger" label={zh ? `删除 ${voice.voiceId}` : `Delete ${voice.voiceId}`} disabled={busyId !== null} onClick={() => void remove(voice)}>
          <Trash2 />
        </IconButton>
      </li>
    })}</ul>}
    <p className="kv-row-desc kv-media-footnote">{zh
      ? '克隆只能使用你有权使用的声音样本。这里是本机记录，供应商可能会让临时声音过期；试听播放已保存的音频，不会重新计费。'
      : 'Only clone voices you are authorized to use. These are local records; providers may expire temporary voices. Playback uses saved audio and is never billed again.'}</p>
  </SettingsGroup>
}
