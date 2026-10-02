import { useEffect, useState } from 'react'
import { Cpu, RefreshCw } from 'lucide-react'
import { api } from '../../api/tauri'
import type { Lang } from '../../components/i18n'
import { IconButton } from '../../components/Button'
import { SettingsGroup } from '../components'

/** Installed system voices come from the same App owner that executes local TTS. */
export function MediaLocalSpeechPanel({ lang }: { lang: Lang }) {
  const zh = lang === 'zh'
  const [voices, setVoices] = useState<string[] | null>(null)
  const [error, setError] = useState('')
  const [reload, setReload] = useState(0)
  useEffect(() => {
    let alive = true
    setVoices(null)
    setError('')
    void api.listLocalSpeechVoices()
      .then(result => { if (alive) setVoices(result) })
      .catch(failure => { if (alive) setError(String(failure)) })
    return () => { alive = false }
  }, [reload])
  const available = voices !== null && voices.length > 0
  return <SettingsGroup title={<div className="kv-media-group-title">
    <span>{zh ? '本地语音' : 'Local speech'}</span>
    <IconButton size="xs" variant="ghost" label={zh ? '刷新本地声音' : 'Refresh system voices'} onClick={() => setReload(value => value + 1)}><RefreshCw /></IconButton>
  </div>}>
    <ul className="kv-media-list"><li className="kv-media-row">
      <span className="kv-media-local-icon" aria-hidden="true"><Cpu size={13} /></span>
      <div className="min-w-0 flex-1">
        <div className="kv-media-row-name">
          <span className="kv-row-label">{zh ? '系统本地 TTS' : 'System TTS'}</span>
          <span className={`kv-tag${available ? ' accent' : ''}`}>{error ? (zh ? '读取失败' : 'Read failed') : voices === null ? (zh ? '正在检测' : 'Checking') : available ? (zh ? '可用' : 'Available') : (zh ? '不可用' : 'Unavailable')}</span>
        </div>
        <p className="kv-row-desc">{zh ? '本地 · system-tts · 离线配音，无需 API Key，输出 WAV。' : 'Local · system-tts · Offline speech, no API key, WAV output.'}</p>
        {available && <p className="kv-row-desc">{zh ? `已安装 ${voices.length} 个系统声音` : `${voices.length} installed system voices`}</p>}
      </div>
    </li></ul>
    {error && <p className="kv-media-alert" role="alert">{error}</p>}
    {voices?.length === 0 && <p className="kv-row-desc kv-media-empty">{zh ? '未检测到可用的系统声音。macOS 和 Windows 可在系统设置中添加声音，再刷新这里。' : 'No installed system voices detected. On macOS or Windows, add voices in system settings and refresh here.'}</p>}
    {available && <details className="kv-row-desc kv-media-empty">
      <summary>{zh ? '查看系统声音' : 'View system voices'}</summary>
      <ul>{voices.map(voice => <li key={voice}>{voice}</li>)}</ul>
    </details>}
    <p className="kv-row-desc kv-media-footnote">{zh ? '可直接选择 local/system-tts 使用。未启用云端语音时默认使用本地声音；系统声音不支持克隆。' : 'Select local/system-tts directly. With no enabled cloud speech model, system speech is the default. System voices do not support cloning.'}</p>
  </SettingsGroup>
}
