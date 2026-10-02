import { useState } from 'react'
import type { ModelInfo } from '../api/tauri'
import { knownAudioModelInfo, SPEECH_ENDPOINTS, TRANSCRIBE_ENDPOINTS } from '../data/speechModels'
import { Button } from '../components/Button'
import { Input, Select } from './public/controls'

const SPEECH_PROTOCOL_LABEL: Record<string, string> = { minimax_tts: 'MiniMax TTS', openai_tts: 'OpenAI TTS' }
const TRANSCRIBE_PROTOCOL_LABEL: Record<string, string> = { openai_transcribe: 'OpenAI transcriptions' }

type Kind = 'speech' | 'transcribe'

/**
 * Connection of an audio model (speech synthesis or cloud transcription). It is independent of the
 * provider's chat connection: a chat key does not prove the speech product is enabled.
 * Models in the catalog start with their protocol and URL; the fields below are only for changing them.
 */
export function AudioModelFields({ model, form, baseUrl, lang, onChange }: {
  model: string
  form: ModelInfo
  baseUrl?: string
  lang: 'zh' | 'en'
  onChange: <K extends keyof ModelInfo>(key: K, value: ModelInfo[K]) => void
}) {
  const zh = lang === 'zh'
  const kind: Kind = form.transcribeProtocol || form.capabilities?.speechTranscription ? 'transcribe' : 'speech'
  const protocol = kind === 'speech' ? form.speechProtocol : form.transcribeProtocol
  const productUrl = kind === 'speech' ? form.speechBaseUrl : form.transcribeBaseUrl
  const known = baseUrl ? knownAudioModelInfo(baseUrl, model) : null
  const [editing, setEditing] = useState(!protocol || !productUrl)
  const labels = kind === 'speech' ? SPEECH_PROTOCOL_LABEL : TRANSCRIBE_PROTOCOL_LABEL
  const catalogProtocols = [...new Set((kind === 'speech' ? SPEECH_ENDPOINTS : TRANSCRIBE_ENDPOINTS).filter(endpoint => endpoint.models.includes(model)).map(endpoint => endpoint.protocol))]
  const title = kind === 'speech' ? (zh ? '语音合成连接' : 'Speech synthesis connection') : (zh ? '云端转写连接' : 'Cloud transcription connection')
  const protocolKey = kind === 'speech' ? 'speechProtocol' : 'transcribeProtocol'
  const urlKey = kind === 'speech' ? 'speechBaseUrl' : 'transcribeBaseUrl'
  const setProtocol = (value: string) => {
    onChange(protocolKey, (value || undefined) as never)
    if (value && !productUrl) {
      const endpoint = (kind === 'speech' ? SPEECH_ENDPOINTS : TRANSCRIBE_ENDPOINTS).find(item => item.protocol === value)
      if (endpoint) onChange(urlKey, endpoint.baseUrl as never)
    }
  }

  return <div className="kv-drawer-section min-w-0">
    <label className="kv-drawer-label">{title}</label>
    <p className="kv-row-desc">{kind === 'speech'
      ? (zh ? '独立于聊天连接。供应商的 Key 不代表已开通语音产品，MiniMax 需要按量付费的语音 Key。' : 'Independent of the chat connection. A provider key does not imply speech access; MiniMax needs a pay-as-you-go speech key.')
      : (zh ? '会把音频上传到该地址，可能产生费用。只在你选用此模型时使用，不会替代本地转写。' : 'Audio is uploaded to this address and may be charged. It is used only when you choose this model and never replaces local transcription.')}</p>
    {!editing && protocol && productUrl
      ? <div className="flex min-w-0 flex-wrap items-center gap-2 pt-1">
        <span className="kv-tag ok">{labels[protocol] ?? protocol}</span>
        <span className="kv-row-desc font-mono [overflow-wrap:anywhere]">{productUrl}</span>
        <Button size="sm" variant="ghost" onClick={() => setEditing(true)}>{zh ? '修改' : 'Edit'}</Button>
      </div>
      : <>
        <Select ariaLabel={kind === 'speech' ? (zh ? '语音协议' : 'Speech protocol') : (zh ? '云转写协议' : 'Cloud transcription protocol')} value={protocol || ''}
          options={[{ value: '', label: zh ? '未启用' : 'Disabled' }, ...(catalogProtocols.length ? catalogProtocols : Object.keys(labels)).map(value => ({ value, label: labels[value] ?? value }))]}
          onChange={setProtocol} />
        <label className="block pt-2"><span className="kv-row-desc">{kind === 'speech' ? (zh ? '语音产品 Base URL' : 'Speech product base URL') : (zh ? '转写产品 Base URL' : 'Transcription product base URL')}</span>
          <Input value={productUrl || ''} onChange={value => onChange(urlKey, (value || undefined) as never)}
            placeholder={known?.[urlKey] ?? 'https://api.openai.com/v1'} mono /></label>
        {known && productUrl !== known[urlKey] && <Button size="sm" variant="ghost" className="mt-1" onClick={() => { onChange(protocolKey, known[protocolKey] as never); onChange(urlKey, known[urlKey] as never) }}>
          {zh ? '恢复官方地址' : 'Use the official address'}
        </Button>}
      </>}
  </div>
}
