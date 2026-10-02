import { useEffect, useRef, useState } from 'react'
import { api, type LocalAsrConfig, type LocalAsrStatus } from '../../api/tauri'
import type { Lang } from '../../components/i18n'
import { Button } from '../../components/Button'
import { SettingsGroup } from '../components'
import { Toggle } from '../public/controls'
import { estimateLocalAsrDownloadGb } from './localAsrDownload'
import { formatBytes } from '../../utils/formatBytes'

/** Languages with a WhisperX default alignment model, most requested first. */
const LANGUAGES: Array<[code: string, zh: string, en: string]> = [
  ['zh', '中文', 'Chinese'], ['en', '英语', 'English'], ['ja', '日语', 'Japanese'], ['ko', '韩语', 'Korean'],
  ['fr', '法语', 'French'], ['de', '德语', 'German'], ['es', '西班牙语', 'Spanish'], ['pt', '葡萄牙语', 'Portuguese'],
  ['ru', '俄语', 'Russian'], ['it', '意大利语', 'Italian'], ['ar', '阿拉伯语', 'Arabic'], ['vi', '越南语', 'Vietnamese'],
]
const STAGES = ['venv', 'dependencies', 'models', 'selfTest'] as const
const STAGE_LABEL: Record<string, [string, string]> = {
  starting: ['准备安装', 'Preparing'],
  venv: ['创建独立运行环境', 'Creating an isolated environment'],
  dependencies: ['安装依赖', 'Installing dependencies'],
  models: ['下载识别与对齐模型', 'Downloading models'],
  selfTest: ['离线自检', 'Running an offline self-test'],
}
const ERROR_LABEL: Record<string, [string, string]> = {
  ASR_INSTALL_CONFLICT: ['已有另一个安装在进行，请等它结束后再试。', 'Another installation is in progress. Try again when it ends.'],
  ASR_INSTALL_CANCELLED: ['安装已取消，原有环境保持不变。', 'Installation cancelled. The previous environment is unchanged.'],
  ASR_INSTALL_STALLED: ['下载长时间没有进展，可能是网络中断。重试会接着已下载的部分继续。', 'The download made no progress for a long time, possibly a network outage. Retrying resumes from what was already downloaded.'],
  ASR_INSTALL_TIMEOUT: ['安装超时。重试会接着已下载的部分继续。', 'Installation timed out. Retrying resumes from what was already downloaded.'],
  ASR_INSTALL_FAILED: ['安装失败，原有环境保持不变。可以重试。', 'Installation failed. The previous environment is unchanged. You can retry.'],
  ASR_SELF_TEST_FAILED: ['安装后自检未通过，原有环境保持不变。', 'The post-install self-test failed. The previous environment is unchanged.'],
  ASR_PYTHON_MISSING: ['缺少内置 Python 运行时，请重新安装 Dsivio。', 'The bundled Python runtime is missing. Reinstall Dsivio.'],
  ASR_BUSY: ['服务正在转写，请先在媒体任务中取消该任务。', 'The service is transcribing. Cancel that task in the media task list first.'],
  ASR_LANGUAGE_INVALID: ['语言代码无效。', 'Invalid language code.'],
}

function describeError(raw: string, zh: boolean): { text: string; detail?: string } {
  const code = raw.match(/ASR_[A-Z_]+/)?.[0]
  const label = code && ERROR_LABEL[code]
  return label ? { text: label[zh ? 0 : 1], detail: raw } : { text: raw }
}

export function LocalAsrPanel({ config, lang, onUpdate }: { config: LocalAsrConfig; lang: Lang; onUpdate?: (config: LocalAsrConfig) => void }) {
  const zh = lang === 'zh'
  const [status, setStatus] = useState<LocalAsrStatus | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [reload, setReload] = useState(0)
  const mounted = useRef(false)
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  const active = status?.state === 'installing' || ['starting', 'stopping', 'busy'].includes(status?.runtime.state ?? '')
  useEffect(() => {
    let alive = true
    let timer: ReturnType<typeof setTimeout>
    async function refresh() {
      try { const next = await api.getLocalAsrStatus(); if (alive) setStatus(next) }
      catch (failure) { if (alive) setError(String(failure)) }
      // Poll quickly only while something is changing; otherwise just keep runtime state roughly fresh.
      finally { if (alive) timer = setTimeout(() => void refresh(), active ? 1500 : 6000) }
    }
    void refresh()
    return () => { alive = false; clearTimeout(timer) }
  }, [reload, active])

  async function act(action: 'install' | 'stop' | 'cancel') {
    if (busy) return
    setBusy(true); setError('')
    try {
      if (action === 'install') setStatus(await api.installLocalAsr(config))
      else if (action === 'cancel' && status?.operationId) setStatus(await api.cancelLocalAsrInstall(status.operationId))
      else if (action === 'stop') await api.stopLocalAsr()
      if (mounted.current) setReload(value => value + 1)
    } catch (failure) { if (mounted.current) setError(String(failure)) }
    finally { if (mounted.current) setBusy(false) }
  }

  const installing = status?.state === 'installing'
  const installed = status?.state === 'ready'
  const languagesChanged = installed && [...config.languages].sort().join() !== [...status.languages].sort().join()
  const toggleLanguage = (code: string) => {
    const languages = config.languages.includes(code) ? config.languages.filter(item => item !== code) : [...config.languages, code]
    if (languages.length > 0) onUpdate?.({ ...config, languages })
  }
  const extraLanguages = config.languages.filter(code => !LANGUAGES.some(([known]) => known === code))
  const downloadGb = estimateLocalAsrDownloadGb(config.languages).toFixed(1)
  const stageIndex = status?.progress ? STAGES.indexOf(status.progress.stage as typeof STAGES[number]) : -1
  const stateTag = !status ? null : {
    notInstalled: <span className="kv-tag">{zh ? '未安装' : 'Not installed'}</span>,
    installing: <span className="kv-tag accent">{zh ? '安装中' : 'Installing'}</span>,
    ready: <span className="kv-tag ok">{zh ? '已安装' : 'Installed'}</span>,
    failed: <span className="kv-tag danger">{zh ? '安装失败' : 'Install failed'}</span>,
    cancelled: <span className="kv-tag">{zh ? '安装已取消' : 'Cancelled'}</span>,
  }[status.state] ?? <span className="kv-tag">{status.state}</span>
  const runtimeLabel: Record<string, string> = zh
    ? { stopped: '未运行', starting: '启动中', ready: '运行中（空闲）', busy: '正在转写', stopping: '停止中' }
    : { stopped: 'Not running', starting: 'Starting', ready: 'Running (idle)', busy: 'Transcribing', stopping: 'Stopping' }
  const failure = error || status?.error
  const shownError = failure ? describeError(failure, zh) : null
  const installLabel = status?.state === 'failed' ? (zh ? '重试安装' : 'Retry installation')
    : languagesChanged ? (zh ? '按新语言重新安装' : 'Reinstall with new languages')
      : installed ? null : (zh ? '安装' : 'Install')

  return <SettingsGroup title={<div className="kv-media-group-title"><span>{zh ? '本地转写 · WhisperX small' : 'Local transcription · WhisperX small'}</span>{stateTag}</div>}>
    <div className="kv-media-card">
      <p className="kv-row-desc">{zh
        ? `安装时下载依赖和模型（按所选语言约 ${downloadGb} GB，已下载的部分重试或改语言时会复用），之后完全离线识别。安装失败会保留原有环境，不会改用云端。`
        : `Installation downloads dependencies and models (about ${downloadGb} GB for the selected languages; anything already downloaded is reused on retry or language changes). Recognition then runs fully offline. A failed install keeps the previous environment and never falls back to the cloud.`}</p>

      <div className="kv-media-field">
        <div className="kv-row-label">{zh ? '识别语言' : 'Languages'}</div>
        <div className="kv-media-langs" role="group" aria-label={zh ? '识别语言' : 'Languages'}>
          {[...LANGUAGES, ...extraLanguages.map(code => [code, code, code] as [string, string, string])].map(([code, zhName, enName]) => {
            const on = config.languages.includes(code)
            return <button key={code} type="button" className={`kv-media-lang${on ? ' on' : ''}`} aria-pressed={on}
              disabled={!onUpdate || installing || (on && config.languages.length === 1)} onClick={() => toggleLanguage(code)} data-tauri-drag-region="false">
              {zh ? zhName : enName}
            </button>
          })}
        </div>
        {languagesChanged && <p className="kv-row-desc kv-media-warn-text">{zh
          ? `已安装的是 ${status.languages.join('、')}。改动语言后需要重新安装才生效。`
          : `Installed: ${status.languages.join(', ')}. Reinstall for the language change to take effect.`}</p>}
      </div>

      <div className="kv-row">
        <div className="kv-row-text">
          <span className="kv-row-label">{zh ? '首次转写时自动安装' : 'Install automatically on first use'}</span>
          <p className="kv-row-desc">{zh ? '关闭后需要在这里手动安装。' : 'When off, install manually here.'}</p>
        </div>
        <div className="kv-row-control"><Toggle ariaLabel={zh ? '首次转写时自动安装' : 'Install automatically on first use'} checked={config.autoInstall} disabled={!onUpdate}
          onChange={autoInstall => onUpdate?.({ ...config, autoInstall })} /></div>
      </div>

      {installing && <div className="kv-media-progress" role="status">
        <div className="kv-row-label">{STAGE_LABEL[status.progress?.stage ?? 'starting']?.[zh ? 0 : 1] ?? status.progress?.stage}
          {stageIndex >= 0 && <span className="kv-row-desc"> · {zh ? `第 ${stageIndex + 1}/${STAGES.length} 步` : `Step ${stageIndex + 1} of ${STAGES.length}`}</span>}
          {status.progress?.downloadedBytes ? <span className="kv-row-desc"> · {zh ? `本次已下载 ${formatBytes(status.progress.downloadedBytes)}` : `${formatBytes(status.progress.downloadedBytes)} downloaded`}</span> : null}</div>
        <div className="kv-media-progress-bar"><span style={{ width: `${((Math.max(stageIndex, 0) + 0.5) / STAGES.length) * 100}%` }} /></div>
      </div>}

      {status && !installing && <p className="kv-row-desc" role="status">
        {zh ? '服务' : 'Service'}：{runtimeLabel[status.runtime.state] || status.runtime.state}
        {installed && ` · ${zh ? '语言' : 'Languages'} ${status.languages.join(', ')}`}
        <span className="kv-media-faint"> · v{status.serviceVersion}</span>
      </p>}
      {!status && !failure && <p className="kv-row-desc" role="status">{zh ? '正在读取状态…' : 'Loading status…'}</p>}
      {status?.runtime.state === 'busy' && <p className="kv-row-desc">{zh
        ? '正在转写的任务需要在媒体任务列表中取消，停止服务不会中断它。'
        : 'Cancel the active transcription from the media task list; stopping the service never interrupts it.'}</p>}

      {shownError && <div className="kv-media-alert" role="alert">
        <span>{shownError.text}</span>
        {shownError.detail && <span className="kv-media-faint [overflow-wrap:anywhere]">{shownError.detail}</span>}
      </div>}

      {(installLabel || installing || status?.runtime.state === 'ready') && <div className="kv-media-actions">
        {installLabel && <Button size="sm" variant="primary" disabled={busy || installing} onClick={() => void act('install')}>{installLabel}</Button>}
        {installing && <Button size="sm" disabled={busy || !status.operationId} onClick={() => void act('cancel')}>{zh ? '取消安装' : 'Cancel installation'}</Button>}
        {status?.runtime.state === 'ready' && <Button size="sm" disabled={busy} onClick={() => void act('stop')}>{zh ? '停止服务' : 'Stop service'}</Button>}
      </div>}
    </div>
  </SettingsGroup>
}
