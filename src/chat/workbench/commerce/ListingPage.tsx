import { useEffect, useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { RefreshCw } from 'lucide-react'
import { Button, IconButton } from '../../../components/Button'
import { useT, type I18n } from '../../../components/i18n'
import { Input, Select, TextArea } from '../../../settings/public/controls'
import type { Category, CategoryAttribute, ListingRecord, ListingStatus } from '../../../generated/commerce'
import { AssetPicker } from '../content/AssetPicker'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { workbenchHash } from '../workbenchPages'
import { ALL_SHOP_PLATFORMS } from './shopPlatforms'
import {
  applyProduct, buildListingRequest, canRetryListing, emptyListingForm, listingFormFromRecord,
  type ListingFormState, type ListingShopContext,
} from './listingRequest'
import { errorText } from './shopMetricsView'
import { useListing, type AttributeState, type CategoryState } from './useListing'
import { STORE_IMAGE_EXTENSIONS, useFileDrop } from '../useFileDrop'
import './shopOverview.css'

type ListingTab = 'create' | 'running' | 'waiting' | 'done' | 'failed'
type Picker = 'image' | 'text'

function statusLabel(t: I18n, status: ListingStatus): string {
  switch (status) {
    case 'submitting': return t.workbenchListingStatusSubmitting
    case 'reviewing': return t.workbenchListingStatusReviewing
    case 'live': return t.workbenchListingStatusLive
    case 'rejected': return t.workbenchListingStatusRejected
    case 'failed': return t.workbenchListingStatusFailed
    case 'uncertain': return t.workbenchListingStatusUncertain
    case 'banned': return t.workbenchListingStatusBanned
  }
}

function inTab(tab: ListingTab, status: ListingStatus): boolean {
  if (tab === 'create') return true
  if (tab === 'running') return status === 'submitting'
  if (tab === 'waiting') return status === 'reviewing' || status === 'uncertain'
  if (tab === 'done') return status === 'live'
  return status === 'failed' || status === 'rejected' || status === 'banned'
}

function fileName(path: string): string {
  return path.split(/[/\\]/).pop() || path
}

/**
 * Listing form, one persisted record per shop, and status refresh.
 * Product archive only prefills; category is chosen per shop from the platform API.
 */
export function ListingPage() {
  const t = useT()
  const listing = useListing()
  const [tab, setTab] = useState<ListingTab>('create')
  const [form, setForm] = useState<ListingFormState>(emptyListingForm)
  const [editing, setEditing] = useState<ListingRecord | null>(null)
  const [errors, setErrors] = useState<string[]>([])
  const [submitting, setSubmitting] = useState(false)
  const [search, setSearch] = useState('')
  const [archiveId, setArchiveId] = useState('')
  const [picker, setPicker] = useState<Picker | null>(null)
  const [trails, setTrails] = useState<Record<string, string[]>>({})
  const alive = useRef(true)
  useEffect(() => {
    alive.current = true
    return () => { alive.current = false }
  }, [])

  const contexts: ListingShopContext[] = listing.shops.map((shop) => {
    const capability = listing.capabilities[shop.id]
    if (!capability) return { id: shop.id, name: shop.name, listing: null }
    if (capability.state === 'error') return { id: shop.id, name: shop.name, listing: false, note: capability.message }
    return { id: shop.id, name: shop.name, listing: capability.value.listing, note: capability.value.notes[0] }
  })

  const tabs: { id: ListingTab; label: string }[] = [
    { id: 'create', label: t.workbenchListingCreate },
    { id: 'running', label: t.workbenchListingRunning },
    { id: 'waiting', label: t.workbenchListingWaiting },
    { id: 'done', label: t.workbenchListingDone },
    { id: 'failed', label: t.workbenchListingFailed },
  ]
  const query = search.trim().toLowerCase()
  const shown = listing.records.filter((record) => {
    if (!inTab(tab, record.status)) return false
    if (!query) return true
    return record.title.toLowerCase().includes(query) || record.target.categoryId.includes(query)
  })

  function patch(partial: Partial<ListingFormState>) {
    setForm((current) => ({ ...current, ...partial }))
  }

  function toggleShop(shopId: string) {
    if (editing) return
    const shop = contexts.find((item) => item.id === shopId)
    if (!shop || shop.listing !== true) return
    if (form.shopIds.includes(shopId)) {
      patch({ shopIds: form.shopIds.filter((id) => id !== shopId) })
      return
    }
    setTrails((current) => ({ ...current, [shopId]: ['0'] }))
    void listing.loadCategories(shopId, '0')
    patch({ shopIds: [...form.shopIds, shopId] })
  }

  function chooseCategory(shopId: string, categoryId: string) {
    const state = listing.categories[shopId]
    const category = state?.items.find((item) => item.id === categoryId)
    if (category && !category.leaf) {
      setTrails((current) => ({ ...current, [shopId]: [...(current[shopId] ?? ['0']), category.id] }))
      setForm((current) => ({
        ...current,
        categories: { ...current.categories, [shopId]: '' },
        attributes: { ...current.attributes, [shopId]: {} },
      }))
      void listing.loadCategories(shopId, category.id)
      return
    }
    setForm((current) => ({
      ...current,
      categories: { ...current.categories, [shopId]: categoryId },
      attributes: { ...current.attributes, [shopId]: category && current.categories[shopId] === categoryId ? current.attributes[shopId] ?? {} : {} },
    }))
    if (categoryId) void listing.loadAttributes(shopId, categoryId)
  }

  function goBack(shopId: string) {
    const next = (trails[shopId] ?? ['0']).slice(0, -1)
    const parent = next[next.length - 1] ?? '0'
    setTrails((current) => ({ ...current, [shopId]: next.length ? next : ['0'] }))
    setForm((current) => ({
      ...current,
      categories: { ...current.categories, [shopId]: '' },
      attributes: { ...current.attributes, [shopId]: {} },
    }))
    void listing.loadCategories(shopId, parent)
  }

  function setAttribute(shopId: string, attributeId: string, value: string, values: string[]) {
    setForm((current) => ({
      ...current,
      attributes: {
        ...current.attributes,
        [shopId]: { ...current.attributes[shopId], [attributeId]: { value, values } },
      },
    }))
  }

  function addImages(paths: string[]) {
    setForm((current) => {
      const images = [...current.images]
      for (const path of paths) {
        if (images.length >= 9) break
        if (!images.includes(path)) images.push(path)
      }
      return { ...current, images }
    })
    if (paths.length && form.images.length + paths.length > 9) setErrors(['请提供 1 到 9 张图片的绝对路径'])
  }

  async function pickFiles() {
    try {
      const picked = await open({
        multiple: true,
        directory: false,
        filters: [{ name: 'Image', extensions: [...STORE_IMAGE_EXTENSIONS] }],
      })
      if (!alive.current || picked == null) return
      addImages(Array.isArray(picked) ? picked : [picked])
    } catch (err) {
      if (alive.current) setErrors([errorText(err)])
    }
  }

  const imageZone = useRef<HTMLDivElement>(null)
  const imageOver = useFileDrop(imageZone, STORE_IMAGE_EXTENSIONS, (accepted, rejected) => {
    if (accepted.length > 0) addImages(accepted)
    else if (rejected.length > 0) setErrors([`${t.workbenchDropUnsupported}${STORE_IMAGE_EXTENSIONS.join(' / ')}`])
  })

  function beginEdit(record: ListingRecord) {
    setEditing(record)
    setForm(listingFormFromRecord(record))
    setTab('create')
    setErrors([])
    setArchiveId('')
    setTrails({ [record.shopId]: ['0'] })
    void listing.loadCategories(record.shopId, '0')
    void listing.loadAttributes(record.shopId, record.target.categoryId)
  }

  async function onSubmit() {
    const defs: Record<string, CategoryAttribute[] | 'loading' | 'error' | undefined> = {}
    for (const shopId of form.shopIds) {
      const categoryId = form.categories[shopId]
      if (!categoryId) continue
      const state = listing.attributes[shopId]
      if (!state || state.loading || state.categoryId !== categoryId) defs[shopId] = 'loading'
      else if (state.error) defs[shopId] = 'error'
      else defs[shopId] = state.items
    }
    const built = buildListingRequest(form, contexts, defs)
    if (!built.ok) {
      setErrors(built.errors)
      return
    }
    setSubmitting(true)
    setErrors([])
    try {
      if (editing) {
        await listing.resubmit(editing, built.draft, built.targets[0] ?? null)
        if (!alive.current) return
        setEditing(null)
      } else {
        await listing.submit(built.draft, built.targets)
        if (!alive.current) return
      }
      setForm(emptyListingForm())
      setArchiveId('')
    } catch (err) {
      if (alive.current) setErrors([errorText(err)])
    } finally {
      if (alive.current) setSubmitting(false)
    }
  }

  async function onRetry(record: ListingRecord) {
    setErrors([])
    try {
      await listing.resubmit(record, null, null)
    } catch (err) {
      if (alive.current) setErrors([errorText(err)])
    }
  }

  async function onRefresh(record: ListingRecord) {
    setErrors([])
    try {
      await listing.refreshRecord(record.id)
    } catch (err) {
      if (alive.current) setErrors([errorText(err)])
    }
  }

  return (
    <WorkbenchPage
      className="listing-page"
      crumb={t.workbenchGroupCommerce}
      title={t.workbenchNavListing}
      error={listing.shopError || listing.listError || errors.join('；')}
      onErrorDismiss={() => setErrors([])}
      actions={(
        <>
          <IconButton label={t.workbenchRefresh} size="sm" onClick={() => { listing.refreshShops(); void listing.refreshRecords() }}>
            <RefreshCw size={14} />
          </IconButton>
          <Button size="sm" onClick={() => { window.location.hash = workbenchHash('shops') }}>{t.workbenchShopsBindTitle}</Button>
        </>
      )}
    >

      {tab === 'create' ? (
        <WorkbenchCard title={editing ? t.workbenchListingEditing : t.workbenchListingCreate}>
          <label className="workbench-field">
            <span>{t.workbenchListingArchive}</span>
            <Select
              ariaLabel={t.workbenchListingArchive}
              value={archiveId}
              onChange={(id) => {
                setArchiveId(id)
                const product = listing.products.find((item) => item.id === id)
                if (product) setForm((current) => applyProduct(current, product))
              }}
              options={[{ value: '', label: t.workbenchListingNoArchive }, ...listing.products.map((product) => ({ value: product.id, label: product.name }))]}
            />
            {listing.productError ? <span className="workbench-page-sub">{listing.productError}</span> : null}
          </label>
          <label className="workbench-field">
            <span>{t.workbenchListingTitle}</span>
            <Input aria-label={t.workbenchListingTitle} value={form.title} onChange={(title) => patch({ title })} />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchListingDesc}</span>
            <TextArea value={form.description} onChange={(description) => patch({ description })} rows={4} />
          </label>
          <div className="listing-shop-row">
            <Button size="sm" onClick={() => setPicker('text')}>{t.workbenchListingPickText}</Button>
          </div>
          <label className="workbench-field">
            <span>{t.workbenchListingPrice}</span>
            <Input aria-label={t.workbenchListingPrice} value={form.price} onChange={(price) => patch({ price })} inputMode="decimal" />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchListingCurrency}</span>
            <Input aria-label={t.workbenchListingCurrency} value={form.currency} onChange={(currency) => patch({ currency })} />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchListingStock}</span>
            <Input aria-label={t.workbenchListingStock} value={form.stock} onChange={(stock) => patch({ stock })} inputMode="numeric" />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchListingSku}</span>
            <Input aria-label={t.workbenchListingSku} value={form.sku} onChange={(sku) => patch({ sku })} />
          </label>
          <div ref={imageZone} className={`workbench-field workbench-drop-zone${imageOver ? ' is-drop-over' : ''}`}>
            <span>{t.workbenchListingImages}</span>
            <div className="listing-image-row">
              {form.images.map((path) => (
                <span key={path}>
                  {fileName(path)}
                  <Button size="sm" aria-label={`${t.workbenchListingRemoveImage} ${fileName(path)}`} onClick={() => patch({ images: form.images.filter((item) => item !== path) })}>
                    {t.workbenchUploadRemove}
                  </Button>
                </span>
              ))}
            </div>
            <div className="listing-shop-row">
              <Button size="sm" onClick={() => void pickFiles()}>{t.workbenchListingPickFile}</Button>
              <Button size="sm" onClick={() => setPicker('image')}>{t.workbenchListingPickAsset}</Button>
            </div>
          </div>
          <label className="workbench-field">
            <span>{t.workbenchListingWeight}</span>
            <Input aria-label={t.workbenchListingWeight} value={form.weightKg} onChange={(weightKg) => patch({ weightKg })} inputMode="decimal" />
          </label>
          <div className="workbench-field">
            <span>{t.workbenchListingDimensions}</span>
            <div className="listing-shop-row">
              <Input aria-label={t.workbenchListingLength} value={form.lengthCm} onChange={(lengthCm) => patch({ lengthCm })} inputMode="decimal" />
              <Input aria-label={t.workbenchListingWidth} value={form.widthCm} onChange={(widthCm) => patch({ widthCm })} inputMode="decimal" />
              <Input aria-label={t.workbenchListingHeight} value={form.heightCm} onChange={(heightCm) => patch({ heightCm })} inputMode="decimal" />
            </div>
          </div>
          <label className="workbench-field">
            <span>{t.workbenchListingBrand}</span>
            <Input aria-label={t.workbenchListingBrand} value={form.brand} onChange={(brand) => patch({ brand })} />
          </label>

          <div className="workbench-field">
            <span>{t.workbenchListingPlatforms}</span>
            <div className="listing-shop-row">
              {(editing ? listing.shops.filter((shop) => shop.id === editing.shopId) : listing.shops).map((shop) => {
                const platform = ALL_SHOP_PLATFORMS.find((item) => item.id === shop.platform)?.name ?? shop.platform
                const context = contexts.find((item) => item.id === shop.id)
                return (
                  <span key={shop.id}>
                    {/* ui-guard-ignore:raw-primitive -- 店铺多选沿用工作台 chip。 */}
                    <button
                      type="button"
                      className={`workbench-chip${form.shopIds.includes(shop.id) ? ' is-active' : ''}`}
                      aria-pressed={form.shopIds.includes(shop.id)}
                      disabled={context?.listing !== true || editing != null}
                      onClick={() => toggleShop(shop.id)}
                    >
                      {shop.name}
                    </button>
                    <span className="workbench-page-sub">{platform}{context?.listing === false && context.note ? ` · ${context.note}` : ''}</span>
                  </span>
                )
              })}
            </div>
          </div>

          {form.shopIds.map((shopId) => {
            const shop = listing.shops.find((item) => item.id === shopId)
            const name = shop?.name ?? shopId
            const trail = trails[shopId] ?? ['0']
            const categoryState: CategoryState | undefined = listing.categories[shopId]
            const parent = trail[trail.length - 1] ?? '0'
            const attributeState: AttributeState | undefined = listing.attributes[shopId]
            const selectedCategory = form.categories[shopId] ?? ''
            const items = categoryState && categoryState.parentId === parent ? categoryState.items : []
            const options = [
              { value: '', label: t.workbenchListingChoose },
              ...(selectedCategory && !items.some((item) => item.id === selectedCategory) ? [{ value: selectedCategory, label: selectedCategory }] : []),
              ...items.map((item: Category) => ({ value: item.id, label: item.leaf ? item.name : `${item.name} /` })),
            ]
            return (
              <div key={shopId} className="workbench-field">
                <span>{t.workbenchListingColCategory} · {name}</span>
                {trail.length > 1 ? (
                  <Button size="sm" aria-label={`${t.workbenchListingBack} ${name}`} onClick={() => goBack(shopId)}>{t.workbenchListingBack}</Button>
                ) : null}
                {categoryState?.error ? <p className="workbench-inline-note" role="alert">{categoryState.error}</p> : null}
                {categoryState?.loading || (categoryState && categoryState.parentId !== parent) ? <p className="workbench-page-sub">{t.workbenchListingCategoryWait}</p> : null}
                <Select ariaLabel={`${t.workbenchListingColCategory} ${name}`} value={selectedCategory} options={options} onChange={(id) => chooseCategory(shopId, id)} />
                <AttributeFields
                  shopId={shopId}
                  shopName={name}
                  categoryId={selectedCategory}
                  state={attributeState}
                  values={form.attributes[shopId] ?? {}}
                  onChange={setAttribute}
                />
              </div>
            )
          })}

          <div className="listing-shop-row">
            {editing ? <Button size="sm" onClick={() => { setEditing(null); setForm(emptyListingForm()); setErrors([]) }}>{t.workbenchListingCancelEdit}</Button> : null}
            <Button variant="primary" disabled={submitting} onClick={() => void onSubmit()}>{t.workbenchListingSubmit}</Button>
          </div>
        </WorkbenchCard>
      ) : null}

      <WorkbenchCard title={t.workbenchListingRecords}>
        <div className="custom-scrollbar workbench-tabs workbench-tabs--scroll">
          {tabs.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`workbench-tab${tab === item.id ? ' is-active' : ''}`}
              aria-pressed={tab === item.id}
              onClick={() => setTab(item.id)}
            >
              {item.label}
              <span className="workbench-tab-count">{listing.records.filter((record) => inTab(item.id, record.status)).length}</span>
            </button>
          ))}
        </div>
        <div className="workbench-toolbar">
          <input className="workbench-search" type="search" aria-label={t.workbenchListingSearch} placeholder={t.workbenchListingSearch} value={search} onChange={(event) => setSearch(event.target.value)} />
        </div>
        {shown.length === 0 ? <WorkbenchEmpty compact>{t.workbenchListingEmpty}</WorkbenchEmpty> : (
          <div className="custom-scrollbar workbench-table-scroll">
            <table className="workbench-table">
              <thead>
                <tr>
                  <th>{t.workbenchListingTitle}</th>
                  <th>{t.workbenchListingColShop}</th>
                  <th>{t.workbenchListingColCategory}</th>
                  <th>{t.workbenchShopsColStatus}</th>
                  <th>{t.workbenchListingColReason}</th>
                  <th>{t.workbenchColAction}</th>
                </tr>
              </thead>
              <tbody>
                {shown.map((record) => {
                  const shop = listing.shops.find((item) => item.id === record.shopId)
                  return (
                    <tr key={record.id}>
                      <td>{record.title}</td>
                      <td>{shop?.name ?? record.shopId}</td>
                      <td>{record.target.categoryId}</td>
                      <td>{statusLabel(t, record.status)}</td>
                      <td>{record.reason || t.workbenchMetricEmpty}</td>
                      <td>
                        {canRetryListing(record.status) ? (
                          <>
                            <Button size="sm" aria-label={`${t.workbenchListingRetry} ${record.title}`} onClick={() => void onRetry(record)}>{t.workbenchListingRetry}</Button>
                            <Button size="sm" aria-label={`${t.workbenchListingEdit} ${record.title}`} onClick={() => beginEdit(record)}>{t.workbenchListingEdit}</Button>
                          </>
                        ) : null}
                        <Button size="sm" aria-label={`${t.workbenchListingRefreshStatus} ${record.title}`} onClick={() => void onRefresh(record)}>{t.workbenchListingRefreshStatus}</Button>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        )}
      </WorkbenchCard>

      {picker ? (
        <AssetPicker
          accept={[picker]}
          multiple={picker === 'image'}
          selectedPaths={picker === 'image' ? form.images : []}
          onClose={() => setPicker(null)}
          onPick={(assets) => {
            if (picker === 'image') {
              setForm((current) => {
                const images = [...current.images]
                for (const asset of assets) {
                  if (images.length >= 9) break
                  if (asset.path && !images.includes(asset.path)) images.push(asset.path)
                }
                return { ...current, images }
              })
            } else {
              const asset = assets[0]
              if (asset) {
                setForm((current) => ({
                  ...current,
                  title: current.title.trim() ? current.title : asset.title,
                  description: asset.text ?? current.description,
                }))
              }
            }
            setPicker(null)
          }}
        />
      ) : null}
    </WorkbenchPage>
  )
}

function AttributeFields({
  shopId, shopName, categoryId, state, values, onChange,
}: {
  shopId: string
  shopName: string
  categoryId: string
  state: AttributeState | undefined
  values: Record<string, { value: string; values: string[] }>
  onChange: (shopId: string, attributeId: string, value: string, values: string[]) => void
}) {
  const t = useT()
  if (!categoryId) return null
  if (!state || state.loading || state.categoryId !== categoryId) return <p className="workbench-page-sub">{t.workbenchListingCategoryWait}</p>
  if (state.error) return <p className="workbench-inline-note" role="alert">{state.error}</p>
  return (
    <>
      {state.items.map((attribute) => {
        const current = values[attribute.id] ?? { value: '', values: [] }
        const label = `${attribute.name}${attribute.unit ? ` (${attribute.unit})` : ''}${attribute.required ? ' *' : ''}`
        if (attribute.input === 'select') {
          return (
            <Select
              key={attribute.id}
              ariaLabel={`${attribute.name} ${shopName}`}
              value={current.value}
              onChange={(value) => onChange(shopId, attribute.id, value, [])}
              options={[{ value: '', label: t.workbenchListingChoose }, ...attribute.options.map((option) => ({ value: option.id, label: option.name }))]}
            />
          )
        }
        if (attribute.input === 'multiSelect') {
          return (
            <div key={attribute.id} className="listing-shop-row">
              {attribute.options.map((option) => {
                const on = current.values.includes(option.id) || current.values.includes(option.name)
                return (
                  <button
                    key={option.id}
                    type="button"
                    className={`workbench-chip${on ? ' is-active' : ''}`}
                    aria-pressed={on}
                    onClick={() => onChange(
                      shopId,
                      attribute.id,
                      '',
                      on ? current.values.filter((token) => token !== option.id && token !== option.name) : [...current.values, option.id],
                    )}
                  >
                    {option.name}
                  </button>
                )
              })}
            </div>
          )
        }
        return (
          <label key={attribute.id} className="workbench-field">
            <span>{label}</span>
            <Input
              aria-label={`${attribute.name} ${shopName}`}
              value={current.value}
              inputMode={attribute.input === 'number' ? 'decimal' : 'text'}
              onChange={(value) => onChange(shopId, attribute.id, value, [])}
            />
          </label>
        )
      })}
    </>
  )
}
