import { useEffect, useState } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Input, Select } from '../../../settings/public/controls'
import { api, isTauriRuntime, type ShopAppConfig, type ShopConnection, type ShopPlatform } from '../../../api/tauri'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { SHOP_PLATFORMS, UPCOMING_DOMESTIC_SHOP_PLATFORMS } from './shopPlatforms'
import { ShopPlatformLogo } from './ShopPlatformLogo'
import './shopBinding.css'

const REGIONS = [
  { value: 'AR', label: 'Argentina' }, { value: 'BR', label: 'Brasil' },
  { value: 'MX', label: 'México' }, { value: 'CL', label: 'Chile' },
  { value: 'CO', label: 'Colombia' }, { value: 'UY', label: 'Uruguay' },
]
const emptyConfig = (platform: ShopPlatform): ShopAppConfig =>
  ({ platform, appId: '', appSecret: '', redirectUrl: '', authorizeUrl: '', region: 'MX', pkce: false })
const errorText = (error: unknown) => error instanceof Error ? error.message : String(error)

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
      setRequestId(result.requestId); setAuthUrl(result.url)
      await api.openExternal(result.url)
    } catch (error) { setNotice(errorText(error)) }
    finally { setBusy(false) }
  }
  const complete = async () => {
    if (!requestId || busy) return
    setBusy(true); setNotice('')
    try {
      const bound = await api.shopComplete(requestId, callbackUrl)
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
  const name = SHOP_PLATFORMS.find(item => item.id === config?.platform)?.name ?? ''
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
          {t.workbenchShopsDomestic}<span className="workbench-tab-count">{UPCOMING_DOMESTIC_SHOP_PLATFORMS.length}</span>
        </button>
      </div>
      {catalogGroup === 'overseas' ? <div className="workbench-platform-grid shop-binding-platform-grid">{SHOP_PLATFORMS.map(item => <div key={item.id} className={`workbench-platform-card shop-binding-platform-card${config?.platform === item.id ? ' is-selected' : ''}`}>
        <ShopPlatformLogo platform={item.id} />
        <span className="workbench-platform-name">{item.name}</span>
        <div className="shop-binding-platform-action"><Button onClick={() => start(item.id)}>{t.workbenchShopsBindNow}</Button></div>
      </div>)}</div> : <>
        <p className="workbench-page-sub shop-binding-catalog-hint">{t.workbenchShopsDomesticHint}</p>
        <div className="shop-binding-platform-grid shop-binding-platform-grid--planned">{UPCOMING_DOMESTIC_SHOP_PLATFORMS.map(item => <div key={item.id} className="workbench-platform-card shop-binding-platform-card shop-binding-platform-card--planned">
          <span className="shop-binding-planned-mark" aria-hidden="true">{item.mark}</span>
          <span className="workbench-platform-name">{item.name}</span>
          <span className="shop-binding-planned-status">{t.workbenchShopsPlanned}</span>
        </div>)}</div>
      </>}
      {catalogGroup === 'overseas' && config && <div className="shop-bind-form">
        <div className="shop-bind-form-head"><strong>{name}</strong><Button size="sm" onClick={() => setConfig(null)}>{t.workbenchShopsCancel}</Button></div>
        {!requestId ? <>
          <p className="workbench-page-sub">{t.workbenchShopsCredentialHint}</p>
          <label>{config.platform === 'shopee' ? 'Partner ID' : config.platform === 'mercadolibre' ? 'Client ID' : 'App ID / Key'}
            <Input value={config.appId} onChange={appId => setConfig({ ...config, appId })} autoComplete="off" mono /></label>
          <label>{config.platform === 'shopee' ? 'Partner Key' : config.platform === 'mercadolibre' ? 'Client Secret' : 'App Secret'}
            <Input type="password" value={config.appSecret} onChange={appSecret => setConfig({ ...config, appSecret })} autoComplete="off" mono /></label>
          <label>{t.workbenchShopsRedirectUrl}
            <Input value={config.redirectUrl} onChange={redirectUrl => setConfig({ ...config, redirectUrl })} placeholder="https://example.com/callback" autoComplete="off" mono /></label>
          {config.platform === 'tiktok' && <label>{t.workbenchShopsTikTokAuthUrl}
            <Input value={config.authorizeUrl} onChange={authorizeUrl => setConfig({ ...config, authorizeUrl })} placeholder="https://services.tiktokshop.com/open/authorize?..." autoComplete="off" mono /></label>}
          {config.platform === 'mercadolibre' && <label>{t.workbenchShopsRegion}
            <Select value={config.region} onChange={region => setConfig({ ...config, region })} options={REGIONS} /></label>}
          {config.platform === 'mercadolibre' && <label>{t.workbenchShopsPkce}
            <Select value={config.pkce ? 'enabled' : 'disabled'} onChange={value => setConfig({ ...config, pkce: value === 'enabled' })}
              options={[{ value: 'disabled', label: t.workbenchShopsPkceDisabled }, { value: 'enabled', label: t.workbenchShopsPkceEnabled }]} /></label>}
          <Button size="sm" variant="primary" disabled={busy || !config.appId.trim() || !config.appSecret.trim() || !config.redirectUrl.trim()} onClick={begin}>
            {busy ? t.workbenchShopsWorking : t.workbenchShopsOpenAuthorization}</Button>
        </> : <>
          <p className="workbench-page-sub">{t.workbenchShopsCallbackHint}</p>
          <Button size="sm" onClick={() => void api.openExternal(authUrl).catch(error => setNotice(errorText(error)))}>{t.workbenchShopsReopenAuthorization}</Button>
          <label>{t.workbenchShopsCallbackUrl}
            <Input value={callbackUrl} onChange={setCallbackUrl} placeholder={config.redirectUrl} autoComplete="off" mono /></label>
          <Button size="sm" variant="primary" disabled={busy || !callbackUrl.trim()} onClick={complete}>
            {busy ? t.workbenchShopsWorking : t.workbenchShopsFinishBinding}</Button>
        </>}
      </div>}
      {notice && <p className="workbench-inline-note" role="alert">{notice}</p>}
    </WorkbenchCard>
    <WorkbenchCard title={t.workbenchShopsBoundTitle} extra={<span className="workbench-page-sub">{t.workbenchShopsBoundCount.replace('{n}', String(shops.length))}</span>}>
      {shops.length > 0 && <div className="workbench-tabs">{SHOP_PLATFORMS.map(item => <button key={item.id} type="button" className={`workbench-tab${platform === item.id ? ' is-active' : ''}`} onClick={() => setPlatform(item.id)}>
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
