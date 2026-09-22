import { useState, type ReactNode } from 'react'
import { Package } from 'lucide-react'
import { api } from '../../../api/tauri'
import type { PickDraft } from '../../../generated/sourcing'
import { useLang } from '../../../components/i18n'
import './sourcing.css'

export function ProductCard({ product, children, details }: {
  product: Pick<PickDraft, 'title' | 'url' | 'imageUrl' | 'price' | 'currency' | 'supplier'>
  children: ReactNode
  details?: ReactNode
}) {
  const en = useLang() === 'en'
  const [failed, setFailed] = useState(false)
  const [error, setError] = useState('')
  return <article className="sourcing-product">
    <div className="sourcing-product-image">
      {product.imageUrl && !failed ? <img src={product.imageUrl} alt={product.title} loading="lazy" referrerPolicy="no-referrer" onError={() => setFailed(true)} /> : <Package size={28} aria-hidden />}
    </div>
    <div className="sourcing-product-body">
      <h3 title={product.title}>{product.url ? <a href={product.url} target="_blank" rel="noreferrer" onClick={event => {
        event.preventDefault(); void api.openExternal(product.url!).catch(e => setError(String(e)))
      }}>{product.title}</a> : product.title}</h3>
      <strong>{product.price != null ? `${product.currency || ''} ${product.price}` : en ? 'Price unavailable' : '暂无报价'}</strong>
      {product.supplier && <p>{product.supplier}</p>}
      {details}
      {error && <p role="alert">{error}</p>}
      <div className="sourcing-product-actions">{children}</div>
    </div>
  </article>
}
