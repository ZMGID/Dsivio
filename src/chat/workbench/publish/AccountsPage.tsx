import { useEffect, useRef, useState, type ReactNode } from 'react'
import { X } from 'lucide-react'
import { Button, IconButton } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Input, Select } from '../../../settings/public/controls'
import { api, isTauriRuntime, type PublishAccount, type PublishAppConfig } from '../../../api/tauri'
import type { AccountStatus, ContentPlatform } from '../../../generated/publish'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { ACCOUNT_KPIS } from './publishCatalog'

const emptyConfig = (platform: ContentPlatform): PublishAppConfig => ({ platform, clientId: '', clientSecret: '', redirectUri: '' })
const errorText = (error: unknown) => error instanceof Error ? error.message : String(error)

function BindDialog({ title, busy, onClose, children }: { title: string; busy: boolean; onClose: () => void; children: ReactNode }) {
  const dialog = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    const trigger = document.activeElement
    if (dialog.current && !dialog.current.open) dialog.current.showModal()
    return () => { if (trigger instanceof HTMLElement && trigger.isConnected) trigger.focus() }
  }, [])
  return (
    <dialog ref={dialog} className="kv-modal custom-scrollbar" aria-label={title} aria-busy={busy}
      onCancel={(event) => { event.preventDefault(); if (!busy) onClose() }}>
      {children}
    </dialog>
  )
}

function statusLabel(t: ReturnType<typeof useT>, status: AccountStatus) {
  if (status === 'connected') return t.workbenchVacctsConnected
  if (status === 'needs_reauthorization') return t.workbenchVacctsNeedsAuth
  return t.workbenchVacctsError
}

/**
 * 账号授权：用户自备 client id / secret。YouTube 可本机回环，TikTok 粘贴回调。
 */
export function AccountsPage() {
  const t = useT()
  const [accounts, setAccounts] = useState<PublishAccount[]>([])
  const [config, setConfig] = useState<PublishAppConfig | null>(null)
  const [requestId, setRequestId] = useState('')
  const [authUrl, setAuthUrl] = useState('')
  const [mode, setMode] = useState('')
  const [callbackUrl, setCallbackUrl] = useState('')
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')

  const load = () => {
    if (!isTauriRuntime()) return
    let active = true
    api.publishListAccounts().then((items) => { if (active) setAccounts(items) }).catch((error) => { if (active) setNotice(errorText(error)) })
    return () => { active = false }
  }
  useEffect(() => load(), [])

  const begin = async () => {
    if (!config || busy) return
    setBusy(true)
    setNotice('')
    try {
      const result = await api.publishBegin(config)
      setRequestId(result.requestId)
      setAuthUrl(result.url)
      setMode(result.mode)
      if (result.url) await api.openExternal(result.url)
      if (result.mode === 'loopback') {
        const account = await api.publishComplete(result.requestId, '')
        setAccounts((previous) => [account, ...previous.filter((item) => item.id !== account.id)])
        setConfig(null)
      }
    } catch (error) {
      setNotice(errorText(error))
    } finally {
      setBusy(false)
    }
  }

  const complete = async () => {
    if (!requestId || busy) return
    setBusy(true)
    setNotice('')
    try {
      const account = await api.publishComplete(requestId, callbackUrl)
      setAccounts((previous) => [account, ...previous.filter((item) => item.id !== account.id)])
      setConfig(null)
      setRequestId('')
      setAuthUrl('')
      setCallbackUrl('')
    } catch (error) {
      setNotice(errorText(error))
    } finally {
      setBusy(false)
    }
  }

  const refresh = async (id: string) => {
    setBusy(true)
    setNotice('')
    try {
      const account = await api.publishRefreshAccount(id)
      setAccounts((previous) => previous.map((item) => item.id === id ? account : item))
    } catch (error) {
      setNotice(errorText(error))
    } finally {
      setBusy(false)
    }
  }

  const unbind = async (id: string) => {
    setBusy(true)
    setNotice('')
    try {
      await api.publishUnbind(id)
      setAccounts((previous) => previous.filter((item) => item.id !== id))
    } catch (error) {
      setNotice(errorText(error))
    } finally {
      setBusy(false)
    }
  }

  const connected = accounts.filter((account) => account.status === 'connected').length
  const broken = accounts.length - connected
  const fansKnown = accounts.some((account) => account.fans != null)
  const fans = accounts.reduce((sum, account) => sum + (account.fans ?? 0), 0)
  const kpiValue = (id: 'bound' | 'ok' | 'bad' | 'fans') => {
    if (id === 'bound') return String(accounts.length)
    if (id === 'ok') return String(connected)
    if (id === 'bad') return String(broken)
    return fansKnown ? String(fans) : '—'
  }

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupPublish}
      title={t.workbenchNavVaccts}
      subtitle={t.workbenchVacctsSubtitle}
      error={config ? '' : notice}
      onErrorDismiss={() => setNotice('')}
      actions={<Button size="sm" onClick={() => { setConfig(emptyConfig('tiktok')); setRequestId(''); setAuthUrl(''); setNotice('') }}>{t.workbenchVacctsBind}</Button>}
    >
      <div className="workbench-kpi-grid">
        {ACCOUNT_KPIS.map((item) => (
          <div key={item.id} className="workbench-kpi">
            <span className="workbench-kpi-value">{kpiValue(item.id)}</span>
            <span className="workbench-kpi-label">{t[item.label]}</span>
          </div>
        ))}
      </div>
      <WorkbenchCard title={t.workbenchVacctsList}>
        {accounts.length === 0 ? <WorkbenchEmpty compact>{t.workbenchVacctsEmpty}</WorkbenchEmpty> : (
          <div className="custom-scrollbar workbench-table-scroll">
            <table className="workbench-table">
              <thead>
                <tr>
                  <th>{t.workbenchVacctsColPlatform}</th>
                  <th>{t.workbenchVacctsColAccount}</th>
                  <th>{t.workbenchShopsColStatus}</th>
                  <th>{t.workbenchVacctsColFans}</th>
                  <th>{t.workbenchVacctsColSync}</th>
                  <th>{t.workbenchColAction}</th>
                </tr>
              </thead>
              <tbody>
                {accounts.map((account) => (
                  <tr key={account.id}>
                    <td>{account.platform === 'tiktok' ? t.workbenchVacctsPlatformTiktok : t.workbenchVacctsPlatformYoutube}</td>
                    <td>{account.name}</td>
                    <td>{statusLabel(t, account.status)}{account.detail ? ` · ${account.detail}` : ''}</td>
                    <td>{account.fans == null ? '—' : account.fans}</td>
                    <td>{account.checkedAt}</td>
                    <td>
                      <Button size="sm" disabled={busy} onClick={() => { void refresh(account.id) }}>{t.workbenchVacctsRefresh}</Button>
                      <Button size="sm" disabled={busy} onClick={() => { void unbind(account.id) }}>{t.workbenchVacctsUnbind}</Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </WorkbenchCard>
      {config ? (
        <BindDialog title={t.workbenchVacctsBind} busy={busy} onClose={() => { if (!busy) setConfig(null) }}>
          <div className="shop-bind-form-head">
            <strong>{t.workbenchVacctsBind}</strong>
            <IconButton label={t.workbenchShopsClose} size="sm" disabled={busy} onClick={() => { if (!busy) setConfig(null) }}><X size={18} /></IconButton>
          </div>
          <Select
            ariaLabel={t.workbenchVacctsColPlatform}
            value={config.platform}
            options={[
              { value: 'tiktok', label: t.workbenchVacctsPlatformTiktok },
              { value: 'youtube', label: t.workbenchVacctsPlatformYoutube },
            ]}
            onChange={(platform) => setConfig({ ...config, platform: platform as ContentPlatform })}
          />
          <label className="workbench-field">
            <span>{t.workbenchVacctsClientId}</span>
            <Input aria-label={t.workbenchVacctsClientId} value={config.clientId} onChange={(clientId) => setConfig({ ...config, clientId })} />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchVacctsClientSecret}</span>
            <Input aria-label={t.workbenchVacctsClientSecret} type="password" value={config.clientSecret} onChange={(clientSecret) => setConfig({ ...config, clientSecret })} />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchVacctsRedirect}</span>
            <Input aria-label={t.workbenchVacctsRedirect} value={config.redirectUri} onChange={(redirectUri) => setConfig({ ...config, redirectUri })} placeholder={t.workbenchVacctsRedirectHint} />
          </label>
          {authUrl ? <p>{authUrl}</p> : null}
          {requestId && mode !== 'loopback' ? (
            <label className="workbench-field">
              <span>{t.workbenchVacctsCallback}</span>
              <Input aria-label={t.workbenchVacctsCallback} value={callbackUrl} onChange={setCallbackUrl} placeholder={t.workbenchVacctsCallbackHint} />
            </label>
          ) : null}
          <div className="shop-bind-form-actions">
            <Button size="sm" disabled={busy} onClick={() => setConfig(null)}>{t.cancel}</Button>
            {requestId && mode !== 'loopback' ? (
              <Button size="sm" variant="primary" disabled={busy || !callbackUrl.trim()} onClick={() => { void complete() }}>{t.workbenchVacctsFinish}</Button>
            ) : (
              <Button size="sm" variant="primary" disabled={busy} onClick={() => { void begin() }}>{t.workbenchVacctsStart}</Button>
            )}
          </div>
          {notice ? <p className="workbench-inline-note" role="alert">{notice}</p> : null}
        </BindDialog>
      ) : null}
    </WorkbenchPage>
  )
}
