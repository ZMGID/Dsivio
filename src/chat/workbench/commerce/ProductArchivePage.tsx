import { useEffect, useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { convertFileSrc } from '@tauri-apps/api/core'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Input, TextArea } from '../../../settings/public/controls'
import type { Product } from '../../../generated/products'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { buildProductSaveRequest, type ProductFormError, type ProductSkuDraft } from './productForm'
import { useProducts } from './useProducts'
import { STORE_IMAGE_EXTENSIONS, useFileDrop } from '../useFileDrop'
import './productArchive.css'

type Draft = {
  id?: string
  revision?: number
  name: string
  description: string
  currency: string
  price: string
  stock: string
  skus: Array<ProductSkuDraft & { key: string }>
  images: string[]
  imports: string[]
}

function emptyDraft(): Draft {
  return {
    name: '',
    description: '',
    currency: '',
    price: '',
    stock: '',
    skus: [],
    images: [],
    imports: [],
  }
}

function amount(value: number | null): string {
  return value == null ? '' : String(value)
}

function draftFromProduct(product: Product): Draft {
  return {
    id: product.id,
    revision: product.revision,
    name: product.name,
    description: product.description,
    currency: product.currency ?? '',
    price: amount(product.price),
    stock: amount(product.stock),
    skus: product.skus.map((sku, index) => ({
      key: `${product.id}-${index}`,
      code: sku.code,
      name: sku.name,
      price: amount(sku.price),
      stock: amount(sku.stock),
    })),
    images: product.images,
    imports: [],
  }
}

function previewSrc(path: string): string {
  try {
    return convertFileSrc(path)
  } catch {
    return path
  }
}

function matches(product: Product, query: string): boolean {
  const needle = query.trim().toLowerCase()
  if (!needle) return true
  const haystack = [
    product.name,
    product.description,
    product.currency ?? '',
    ...product.skus.flatMap((sku) => [sku.code, sku.name]),
  ].join('\n').toLowerCase()
  return haystack.includes(needle)
}

export function ProductArchivePage() {
  const t = useT()
  const { products, error, refresh, save, remove } = useProducts()
  const [draft, setDraft] = useState<Draft>(emptyDraft)
  const [query, setQuery] = useState('')
  const [notice, setNotice] = useState('')
  const [saving, setSaving] = useState(false)
  const alive = useRef(true)
  const skuKeys = useRef(0)
  useEffect(() => {
    alive.current = true
    return () => { alive.current = false }
  }, [])

  const formError: Record<ProductFormError, string> = {
    price: t.workbenchArchivePriceInvalid,
    stock: t.workbenchArchiveStockInvalid,
    'sku-price': t.workbenchArchivePriceInvalid,
    'sku-stock': t.workbenchArchiveStockInvalid,
    image: t.workbenchArchiveImageInvalid,
  }

  function addImports(paths: string[]) {
    setDraft((current) => {
      const known = new Set([...current.images, ...current.imports])
      const imports = [...current.imports]
      for (const path of paths) {
        if (!known.has(path)) imports.push(path)
      }
      return { ...current, imports }
    })
  }

  async function pickFiles() {
    const picked = await open({
      multiple: true,
      directory: false,
      filters: [{ name: 'Image', extensions: [...STORE_IMAGE_EXTENSIONS] }],
    })
    if (!alive.current || picked == null) return
    addImports(Array.isArray(picked) ? picked : [picked])
  }

  const zone = useRef<HTMLDivElement>(null)
  const over = useFileDrop(zone, STORE_IMAGE_EXTENSIONS, (accepted, rejected) => {
    if (accepted.length > 0) addImports(accepted)
    else if (rejected.length > 0) setNotice(`${t.workbenchDropUnsupported}${STORE_IMAGE_EXTENSIONS.join(' / ')}`)
  }, saving)

  async function onSave() {
    const built = buildProductSaveRequest({
      id: draft.id,
      revision: draft.revision,
      name: draft.name,
      description: draft.description,
      currency: draft.currency,
      price: draft.price,
      stock: draft.stock,
      skus: draft.skus,
      images: [...draft.images, ...draft.imports],
    })
    if (!built.ok) {
      setNotice(formError[built.error])
      return
    }
    setSaving(true)
    setNotice('')
    try {
      const saved = await save(built.request)
      if (!alive.current) return
      setDraft(draftFromProduct(saved))
      setNotice(t.workbenchArchiveSaved)
    } catch (err) {
      if (!alive.current) return
      setNotice(err instanceof Error ? err.message : String(err))
      void refresh()
    } finally {
      if (alive.current) setSaving(false)
    }
  }

  async function onDelete(product: Product) {
    setNotice('')
    try {
      await remove(product.id)
      if (!alive.current) return
      if (draft.id === product.id) setDraft(emptyDraft())
    } catch (err) {
      if (!alive.current) return
      setNotice(err instanceof Error ? err.message : String(err))
    }
  }

  function addSku() {
    const key = `new-${skuKeys.current++}`
    setDraft((current) => ({
      ...current,
      skus: [...current.skus, { key, code: '', name: '', price: '', stock: '' }],
    }))
  }

  const visible = products.filter((product) => matches(product, query))
  const thumbs = [
    ...draft.images.map((path) => ({
      key: path,
      src: previewSrc(path),
      drop: () => setDraft((current) => ({ ...current, images: current.images.filter((item) => item !== path) })),
    })),
    ...draft.imports.map((path) => ({
      key: path,
      src: previewSrc(path),
      drop: () => setDraft((current) => ({ ...current, imports: current.imports.filter((item) => item !== path) })),
    })),
  ]

  return (
    <WorkbenchPage
      className="commerce-product-archive"
      crumb={t.workbenchGroupCommerce}
      title={t.workbenchNavProducts}
      subtitle={t.workbenchArchiveSubtitle}
      error={error}
      actions={<Button size="sm" onClick={() => { setDraft(emptyDraft()); setNotice('') }}>{t.workbenchArchiveNew}</Button>}
    >
      {notice ? <p className="workbench-inline-note">{notice}</p> : null}
      <div className="workbench-split">
        <WorkbenchCard title={t.workbenchNavProducts} extra={<Button size="sm" onClick={() => void refresh()}>{t.workbenchRefresh}</Button>}>
          <label className="workbench-field">
            <Input type="search" aria-label={t.workbenchArchiveSearch} placeholder={t.workbenchArchiveSearch} value={query} onChange={setQuery} />
          </label>
          {products.length === 0 ? (
            <WorkbenchEmpty title={t.workbenchArchiveEmpty}>{t.workbenchArchiveEmptyHint}</WorkbenchEmpty>
          ) : visible.length === 0 ? (
            <WorkbenchEmpty>{t.workbenchArchiveNoMatch}</WorkbenchEmpty>
          ) : (
            <ul className="product-saved-list">
              {visible.map((product) => (
                <li key={product.id}>
                  <button type="button" className="product-saved-open" onClick={() => { setDraft(draftFromProduct(product)); setNotice('') }}>
                    {product.images[0] ? <img src={previewSrc(product.images[0])} alt="" /> : null}
                    <span>{product.name}</span>
                  </button>
                  <Button size="sm" aria-label={`${t.workbenchArchiveDelete} ${product.name}`} onClick={() => void onDelete(product)}>{t.workbenchArchiveDelete}</Button>
                </li>
              ))}
            </ul>
          )}
        </WorkbenchCard>
        <WorkbenchCard title={t.workbenchArchiveEdit}>
          <div ref={zone} className={`workbench-drop-zone${over ? ' is-drop-over' : ''}`}>
          <label className="workbench-field">
            <span>{t.workbenchArchiveName}</span>
            <Input aria-label={t.workbenchArchiveName} value={draft.name} onChange={(name) => setDraft((current) => ({ ...current, name }))} />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchArchiveDesc}</span>
            <TextArea value={draft.description} onChange={(description) => setDraft((current) => ({ ...current, description }))} rows={4} placeholder={t.workbenchArchiveDescHint} />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchArchiveCurrency}</span>
            <Input aria-label={t.workbenchArchiveCurrency} value={draft.currency} onChange={(currency) => setDraft((current) => ({ ...current, currency }))} />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchArchivePrice}</span>
            <Input aria-label={t.workbenchArchivePrice} inputMode="decimal" value={draft.price} onChange={(price) => setDraft((current) => ({ ...current, price }))} />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchArchiveStock}</span>
            <Input aria-label={t.workbenchArchiveStock} inputMode="numeric" value={draft.stock} onChange={(stock) => setDraft((current) => ({ ...current, stock }))} />
          </label>
          <div className="product-sku">
            <span>{t.workbenchArchiveSku}</span>
            {draft.skus.map((sku) => (
              <div key={sku.key}>
                <label className="workbench-field">
                  <span>{t.workbenchArchiveSkuCode}</span>
                  <Input aria-label={t.workbenchArchiveSkuCode} value={sku.code} onChange={(code) => setDraft((current) => ({
                    ...current,
                    skus: current.skus.map((item) => item.key === sku.key ? { ...item, code } : item),
                  }))} />
                </label>
                <label className="workbench-field">
                  <span>{t.workbenchArchiveSkuName}</span>
                  <Input aria-label={t.workbenchArchiveSkuName} value={sku.name} onChange={(name) => setDraft((current) => ({
                    ...current,
                    skus: current.skus.map((item) => item.key === sku.key ? { ...item, name } : item),
                  }))} />
                </label>
                <label className="workbench-field">
                  <span>{t.workbenchArchiveSkuPrice}</span>
                  <Input aria-label={t.workbenchArchiveSkuPrice} inputMode="decimal" value={sku.price} onChange={(price) => setDraft((current) => ({
                    ...current,
                    skus: current.skus.map((item) => item.key === sku.key ? { ...item, price } : item),
                  }))} />
                </label>
                <label className="workbench-field">
                  <span>{t.workbenchArchiveSkuStock}</span>
                  <Input aria-label={t.workbenchArchiveSkuStock} inputMode="numeric" value={sku.stock} onChange={(stock) => setDraft((current) => ({
                    ...current,
                    skus: current.skus.map((item) => item.key === sku.key ? { ...item, stock } : item),
                  }))} />
                </label>
                <Button size="sm" aria-label={`${t.workbenchArchiveSkuRemove} ${sku.code || sku.name}`} onClick={() => setDraft((current) => ({
                  ...current,
                  skus: current.skus.filter((item) => item.key !== sku.key),
                }))}>{t.workbenchArchiveSkuRemove}</Button>
              </div>
            ))}
            <Button size="sm" onClick={addSku}>{t.workbenchArchiveSkuAdd}</Button>
          </div>
          <div className="product-draft-images">
            {thumbs.map((thumb) => (
              <figure key={thumb.key}>
                <img src={thumb.src} alt="" />
                <Button size="sm" onClick={thumb.drop}>{t.workbenchUploadRemove}</Button>
              </figure>
            ))}
          </div>
          <div className="workbench-cta-row">
            <Button size="sm" onClick={() => void pickFiles()}>{t.workbenchArchivePickImages}</Button>
            <Button variant="primary" disabled={saving || !draft.name.trim()} onClick={() => void onSave()}>{t.workbenchArchiveSave}</Button>
          </div>
          </div>
        </WorkbenchCard>
      </div>
    </WorkbenchPage>
  )
}
