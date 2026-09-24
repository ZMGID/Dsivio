import { useEffect, useRef, useState, type ReactNode } from 'react'
import { X } from 'lucide-react'
import { Button, IconButton } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Input, Select } from '../../../settings/public/controls'
import { api, isTauriRuntime, type ShopAppConfig, type ShopConnection, type ShopPlatform } from '../../../api/tauri'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { ALL_SHOP_PLATFORMS, DOMESTIC_SHOP_PLATFORMS, SHOP_PLATFORMS, isDirectBindPlatform } from './shopPlatforms'
import { ShopPlatformLogo } from './ShopPlatformLogo'
import './shopBinding.css'

const REGIONS = [
  { value: 'AR', label: 'Argentina' }, { value: 'BR', label: 'Brasil' },
  { value: 'MX', label: 'México' }, { value: 'CL', label: 'Chile' },
  { value: 'CO', label: 'Colombia' }, { value: 'UY', label: 'Uruguay' },
]
// 平台对开发者凭证的官方叫法，缺省回退 App ID / Key。
const CREDENTIAL_LABELS: Partial<Record<ShopPlatform, readonly [string, string]>> = {
  shopee: ['Partner ID', 'Partner Key'],
  mercadolibre: ['Client ID', 'Client Secret'],
  pinduoduo: ['Client ID', 'Client Secret'],
  douyin: ['App Key', 'App Secret'],
  kuaishou: ['App Key', 'App Secret'],
  taobao: ['App Key', 'App Secret'],
  wechat: ['小店 AppID', '小店 AppSecret'],
}
const emptyConfig = (platform: ShopPlatform): ShopAppConfig =>
  ({ platform, appId: '', appSecret: '', redirectUrl: '', authorizeUrl: '', region: 'MX', pkce: false })
const errorText = (error: unknown) => error instanceof Error ? error.message : String(error)

function ShopBindDialog({ title, busy, onClose, children }: {
  title: string
  busy: boolean
  onClose: () => void
  children: ReactNode
}) {
  const dialog = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    const trigger = document.activeElement
    if (dialog.current && !dialog.current.open) dialog.current.showModal()
    return () => { if (trigger instanceof HTMLElement && trigger.isConnected) trigger.focus() }
  }, [])

  return <dialog ref={dialog} className="kv-modal shop-bind-dialog custom-scrollbar" aria-label={title} aria-busy={busy}
    onCancel={event => { event.preventDefault(); if (!busy) onClose() }}>
    {children}
  </dialog>
}

export function ShopBindingPage() {
  const t = useT()
  const [shops, setShops] = useState<ShopConnection[]>([])
  const [platform, setPlatform] = useState<ShopPlatform>('shopee')
  const [catalogGroup, setCatalogGroup] = useState<'overseas' | 'domestic'>('overseas')
  const [config, setConfig] = useState<ShopAppConfig | null>(null)
  const [requestId, setRequestId] = useState('')
  const [authUrl, setAuthUrl] = useState('')
  const [callbackUrl, setCallbackUrl] = useState('')
  const [busy, setBusy] = useState(false)
  const [busyShop, setBusyShop] = useState('')
  const [confirmUnbind, setConfirmUnbind] = useState('')
  const [notice, setNotice] = useState('')

  useEffect(() => {
    if (!isTauriRuntime()) return
    let active = true
    api.shopList().then(items => { if (active) setShops(items) })
      .catch(error => { if (active) setNotice(errorText(error)) })
    return () => { active = false }
  }, [])

  const start = (id: ShopPlatform) => {
    setConfig(emptyConfig(id)); setRequestId(''); setAuthUrl(''); setCallbackUrl(''); setNotice('')
  }
  const begin = async () => {
    if (!config || busy) return
    setBusy(true); setNotice('')
    try {
      const result = await api.shopBegin(config)
      setRequestId(result.requestId)
      // 微信小店无授权页：凭证即授权，直接完成绑定。
      if (!result.url) { await complete(result.requestId); return }
      setAuthUrl(result.url)
      await api.openExternal(result.url)
    } catch (error) { setNotice(errorText(error)) }
    finally { setBusy(false) }
  }
  const complete = async (requestToFinish?: string) => {
    const requestIdToFinish = requestToFinish ?? requestId
    if (!requestIdToFinish || busy) return
    setBusy(true); setNotice('')
    try {
      const bound = await api.shopComplete(requestIdToFinish, callbackUrl)
      setShops(previous => {
        const ids = new Set(bound.map(shop => shop.id))
        return [...bound, ...previous.filter(shop => !ids.has(shop.id))]
      })
      if (config) setPlatform(config.platform)
      setConfig(null); setRequestId(''); setAuthUrl(''); setCallbackUrl('')
    } catch (error) { setNotice(errorText(error)) }
    finally { setBusy(false) }
  }
  const check = async (id: string) => {
    setBusyShop(id); setNotice('')
    try {
      const updated = await api.shopCheck(id)
      setShops(previous => previous.map(shop => shop.id === id ? updated : shop))
    } catch (error) { setNotice(errorText(error)) }
    finally { setBusyShop('') }
  }
  const unbind = async (id: string) => {
    setBusyShop(id); setNotice('')
    try {
      await api.shopUnbind(id)
      setShops(previous => previous.filter(shop => shop.id !== id))
      setConfirmUnbind('')
    } catch (error) { setNotice(errorText(error)) }
    finally { setBusyShop('') }
  }

  const visible = shops.filter(shop => shop.platform === platform)
  const name = ALL_SHOP_PLATFORMS.find(item => item.id === config?.platform)?.name ?? ''
  const status = (shop: ShopConnection) => ({
    connected: t.workbenchShopsStatusConnected,
    disabled: t.workbenchShopsStatusDisabled,
    needs_reauthorization: t.workbenchShopsStatusReauthorize,
    error: t.workbenchShopsStatusError,
  })[shop.status]

  return <WorkbenchPage crumb={t.workbenchGroupCommerce} title={t.workbenchNavShops} subtitle={t.workbenchShopsSubtitle}>
    <WorkbenchCard title={t.workbenchShopsSelectPlatform}>
      <div className="workbench-tabs shop-binding-catalog-tabs" aria-label={t.workbenchShopsSelectPlatform}>
        <button type="button" className={`workbench-tab${catalogGroup === 'overseas' ? ' is-active' : ''}`} aria-pressed={catalogGroup === 'overseas'} onClick={() => setCatalogGroup('overseas')}>
          {t.workbenchShopsOverseas}<span className="workbench-tab-count">{SHOP_PLATFORMS.length}</span>
        </button>
        <button type="button" className={`workbench-tab${catalogGroup === 'domestic' ? ' is-active' : ''}`} aria-pressed={catalogGroup === 'domestic'} onClick={() => setCatalogGroup('domestic')}>
          {t.workbenchShopsDomestic}<span className="workbench-tab-count">{DOMESTIC_SHOP_PLATFORMS.length}</span>
        </button>
      </div>
      <div className="workbench-platform-grid shop-binding-platform-grid">
        {(catalogGroup === 'overseas' ? SHOP_PLATFORMS : DOMESTIC_SHOP_PLATFORMS).map(item => <div key={item.id} className="workbench-platform-card shop-binding-platform-card">
          <ShopPlatformLogo platform={item.id} />
          <span className="workbench-platform-name">{item.name}</span>
          <div className="shop-binding-platform-action"><Button onClick={() => start(item.id)}>{t.workbenchShopsBindNow}</Button></div>
        </div>)}
      </div>
      {!config && notice && <p className="workbench-inline-note" role="alert">{notice}</p>}
    </WorkbenchCard>
    {config && <ShopBindDialog title={name} busy={busy} onClose={() => setConfig(null)}>
      <div className="shop-bind-form">
        <div className="shop-bind-form-head"><h2>{name}</h2><IconButton label={t.workbenchShopsClose} size="sm" disabled={busy} onClick={() => setConfig(null)}><X size={18} /></IconButton></div>
        {!requestId ? <>
          <p className="workbench-page-sub">{isDirectBindPlatform(config.platform) ? t.workbenchShopsDirectHint : t.workbenchShopsCredentialHint}</p>
          <label>{CREDENTIAL_LABELS[config.platform]?.[0] ?? 'App ID / Key'}
            <Input value={config.appId} onChange={appId => setConfig({ ...config, appId })} autoComplete="off" mono /></label>
          <label>{CREDENTIAL_LABELS[config.platform]?.[1] ?? 'App Secret'}
            <Input type="password" value={config.appSecret} onChange={appSecret => setConfig({ ...config, appSecret })} autoComplete="off" mono /></label>
          {!isDirectBindPlatform(config.platform) && <label>{t.workbenchShopsRedirectUrl}
            <Input value={config.redirectUrl} onChange={redirectUrl => setConfig({ ...config, redirectUrl })} placeholder="https://example.com/callback" autoComplete="off" mono /></label>}
          {config.platform === 'tiktok' && <label>{t.workbenchShopsTikTokAuthUrl}
            <Input value={config.authorizeUrl} onChange={authorizeUrl => setConfig({ ...config, authorizeUrl })} placeholder="https://services.tiktokshop.com/open/authorize?..." autoComplete="off" mono /></label>}
          {config.platform === 'mercadolibre' && <label>{t.workbenchShopsRegion}
            <Select value={config.region} onChange={region => setConfig({ ...config, region })} options={REGIONS} /></label>}
          {config.platform === 'mercadolibre' && <label>{t.workbenchShopsPkce}
            <Select value={config.pkce ? 'enabled' : 'disabled'} onChange={value => setConfig({ ...config, pkce: value === 'enabled' })}
              options={[{ value: 'disabled', label: t.workbenchShopsPkceDisabled }, { value: 'enabled', label: t.workbenchShopsPkceEnabled }]} /></label>}
          <div className="shop-bind-form-actions">
            <Button size="sm" disabled={busy} onClick={() => setConfig(null)}>{t.workbenchShopsCancel}</Button>
            <Button size="sm" variant="primary" disabled={busy || !config.appId.trim() || !config.appSecret.trim() || (!isDirectBindPlatform(config.platform) && !config.redirectUrl.trim())} onClick={begin}>
              {busy ? t.workbenchShopsWorking : isDirectBindPlatform(config.platform) ? t.workbenchShopsFinishBinding : t.workbenchShopsOpenAuthorization}</Button>
          </div>
        </> : <>
          <p className="workbench-page-sub">{t.workbenchShopsCallbackHint}</p>
          <Button size="sm" onClick={() => void api.openExternal(authUrl).catch(error => setNotice(errorText(error)))}>{t.workbenchShopsReopenAuthorization}</Button>
          <label>{t.workbenchShopsCallbackUrl}
            <Input value={callbackUrl} onChange={setCallbackUrl} placeholder={config.redirectUrl} autoComplete="off" mono /></label>
          <div className="shop-bind-form-actions">
            <Button size="sm" disabled={busy} onClick={() => setConfig(null)}>{t.workbenchShopsCancel}</Button>
            <Button size="sm" variant="primary" disabled={busy || !callbackUrl.trim()} onClick={() => complete()}>
              {busy ? t.workbenchShopsWorking : t.workbenchShopsFinishBinding}</Button>
          </div>
        </>}
        {notice && <p className="workbench-inline-note" role="alert">{notice}</p>}
      </div>
    </ShopBindDialog>}
    <WorkbenchCard title={t.workbenchShopsBoundTitle} extra={<span className="workbench-page-sub">{t.workbenchShopsBoundCount.replace('{n}', String(shops.length))}</span>}>
      {shops.length > 0 && <div className="workbench-tabs">{ALL_SHOP_PLATFORMS
        // 平台多了以后只列出已有绑定店铺的平台。
        .filter(item => shops.some(shop => shop.platform === item.id))
        .map(item => <button key={item.id} type="button" className={`workbench-tab${platform === item.id ? ' is-active' : ''}`} onClick={() => setPlatform(item.id)}>
          {item.name}<span className="workbench-tab-count">{shops.filter(shop => shop.platform === item.id).length}</span>
        </button>)}</div>}
      {visible.length > 0 ? <div className="workbench-table-scroll custom-scrollbar"><table className="workbench-table">
        <thead><tr><th>{t.workbenchShopsColName}</th><th>{t.workbenchShopsColId}</th><th>{t.workbenchShopsColBoundAt}</th><th>{t.workbenchShopsColStatus}</th><th>{t.workbenchColAction}</th></tr></thead>
        <tbody>{visible.map(shop => <tr key={shop.id}>
          <td>{shop.name}{shop.region && <span className="shop-region">{shop.region}</span>}</td>
          <td>{shop.remoteId}</td><td>{new Date(shop.boundAt).toLocaleString()}</td>
          <td>
            <span title={shop.detail ?? undefined}>{status(shop)}</span>
            <span className="shop-checked-at">{t.workbenchShopsCheckedAt.replace('{time}', new Date(shop.checkedAt).toLocaleString())}</span>
          </td>
          <td><div className="shop-row-actions">
            <Button size="sm" disabled={busyShop === shop.id} onClick={() => void check(shop.id)}>{t.workbenchRefresh}</Button>
            {confirmUnbind === shop.id ? <Button size="sm" disabled={busyShop === shop.id} onClick={() => void unbind(shop.id)}>{t.workbenchShopsConfirmUnbind}</Button>
              : <Button size="sm" disabled={busyShop === shop.id} onClick={() => setConfirmUnbind(shop.id)}>{t.workbenchShopsUnbind}</Button>}
          </div></td>
        </tr>)}</tbody>
      </table></div> : <WorkbenchEmpty compact>{t.workbenchShopsEmpty}</WorkbenchEmpty>}
    </WorkbenchCard>
  </WorkbenchPage>
}
