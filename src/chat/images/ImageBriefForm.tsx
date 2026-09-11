import { useMemo, useState, type DragEvent } from 'react'
import { ArrowRight, FolderOpen, ImagePlus, Layers3, Loader2, Plus, Search, X } from 'lucide-react'
import { Button, IconButton } from '../../components/Button'
import { ImageRatioSelect, ImageResolutionSelect } from './ImageOutputSelect'
import { AssetImage, Field, ImageLanguageSelect } from './StudioPanels'
import type { ImageAsset, ImageBrief, ImageTemplate } from './types'

function templateHaystack(item: ImageTemplate): string {
  return [item.data.name, item.data.category, ...item.data.slots.map((slot) => slot.purpose || '')]
    .join(' ')
    .toLowerCase()
}

function TemplateSearchList({
  choices,
  value,
  busy,
  onChange,
  emptyLabel,
  listLabel = '套图模板',
}: {
  choices: ImageTemplate[]
  value: string
  busy: boolean
  onChange: (id: string) => void
  emptyLabel?: string
  listLabel?: string
}) {
  const [query, setQuery] = useState('')
  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase()
    return needle ? choices.filter((item) => templateHaystack(item).includes(needle)) : choices
  }, [choices, query])
  return (
    <div className="if-template-search">
      <label className="if-template-search-field">
        <Search size={14} strokeWidth={2} aria-hidden />
        <input className="kv-input" type="search" disabled={busy} value={query}
          onChange={(event) => setQuery(event.target.value)} placeholder="搜索模板" aria-label="搜索模板" />
      </label>
      {filtered.length || (emptyLabel && !query.trim()) ? (
        <div className="if-template-list custom-scrollbar" role="listbox" aria-label={listLabel}>
          {emptyLabel && !query.trim() && (
            <button type="button" role="option" aria-selected={!value}
              className={`if-template-option${!value ? ' is-active' : ''}`} disabled={busy}
              onClick={() => onChange('')}>
              <strong>{emptyLabel}</strong>
              <small>按页选择样图，不使用模板</small>
            </button>
          )}
          {filtered.map((item) => {
            const selected = item.id === value
            const purposes = item.data.slots.slice(0, 3).map((slot) => (slot.purpose || '商品图').split('·')[0].trim())
            return (
              <button key={item.id} type="button" role="option" aria-selected={selected}
                className={`if-template-option${selected ? ' is-active' : ''}`} disabled={busy}
                onClick={() => onChange(item.id)}>
                <strong>{item.data.name}</strong>
                <small>{item.data.slots.length} 张 · {purposes.join(' / ')}</small>
              </button>
            )
          })}
        </div>
      ) : (
        <p className="if-hint">{choices.length ? '没有匹配的模板' : '还没有可用模板，可先在「制作模板」中创建，或到模板库导入。'}</p>
      )}
    </div>
  )
}

function imageStartIssue(brief: ImageBrief): string {
  if (brief.feature === 'gen') return brief.requirement.trim() ? '' : '先写下你想生成或修改的内容'
  if (!brief.products.length) return '先添加商品图片或商品文件夹'
  if (brief.products.some(product => !product.assets.length)) return '每款商品至少需要一张图片，请补全素材或移除空商品'
  if (brief.feature === 'smart' && !brief.templateId) return '选择要使用的模板'
  if (brief.feature === 'replace' && !brief.templateId && !brief.workflowInput?.sources.length)
    return '添加要沿用的样图，或选择已有模板'
  if (['design', 'client'].includes(brief.feature) && !brief.requirement.trim())
    return '写下这套图的用途和要求'
  return ''
}

const START_LABELS = {
  gen: '生成图片', replace: '开始换货', smart: '生成套图样品',
  design: '开始设计', client: '开始批量试做', workflow: '制作模板',
}

type Props = {
  configurationIssue?: string
  model?: string
  protocol?: string
  brief: ImageBrief
  templates: ImageTemplate[]
  busy: boolean
  dropActive: boolean
  dropTarget?: 'examples' | 'products' | null
  onChange: (patch: Partial<ImageBrief>) => void
  onImport: (folder: boolean) => void
  onImportExamples: () => void
  onDrop: (event: DragEvent, zone?: 'examples' | 'products') => void
  onStart: () => void
}

export function ImageBriefForm({ configurationIssue, model, protocol, brief, templates, busy, dropActive, dropTarget, onChange, onImport, onImportExamples, onDrop, onStart }: Props) {
  const quick = brief.feature === 'gen'
  const replace = brief.feature === 'replace'
  const batch = brief.feature === 'client'
  const template = templates.find((item) => item.id === brief.templateId)
  const sources = brief.workflowInput?.sources || []
  const choices = templates.filter((item) => item.data.mode === (replace ? 'replace' : 'smart'))
  const issue = configurationIssue || imageStartIssue(brief) || (brief.templateId && !template ? '所选模板已不存在，请重新选择模板' : '')
  const pages = template?.data.slots.length || (replace && sources.length) || brief.count
  const nextStep = quick ? '生成后，可以直接说出你想修改的地方'
    : batch ? '每类先试做最多 2 款，满意后再生成剩余商品'
    : brief.products.length > 2 ? `先试做 2 款，共 ${pages * 2} 张，满意后再生成剩余商品`
    : `本次生成 ${brief.products.length} 款，共 ${pages * brief.products.length} 张，生成后可直接修改`
  const updateTemplate = (id: string) => {
    const selected = templates.find((item) => item.id === id)
    onChange({
      templateId: selected?.id || null,
      products: brief.products.map((product) => ({ ...product, templateId: null })),
      count: selected?.data.slots.length || brief.count,
      language: selected?.data.language || brief.language,
      ratio: selected?.data.output?.ratio || brief.ratio,
      resolution: selected?.data.output?.resolution || brief.resolution,
    })
  }
  const removeAsset = (productId: string, asset: ImageAsset) => {
    onChange({ products: brief.products.flatMap((product) => {
      if (product.id !== productId) return [product]
      const assets = product.assets.filter((item) => item.id !== asset.id)
      return assets.some((item) => !item.name.startsWith('__dsimage_')) ? [{
        ...product, assets, front: product.front === asset.id ? null : product.front,
        back: product.back === asset.id ? null : product.back,
      }] : []
    }) })
  }
  return (
    <div className={`if-form${replace || brief.feature === 'smart' ? ' if-form--split' : ''}`}>
      {(replace || brief.feature === 'smart') && (
        <section className="if-template-choice" aria-label={replace ? '要沿用的样图' : '选择套图模板'}>
          <div className="if-section-title">
            <h3>{replace ? '要沿用的样图' : '选择一套模板'}</h3>
            {replace && <span>保留版式，换成你的商品</span>}
          </div>
          {replace && (
            <div
              className={`if-replace-source${dropActive && dropTarget === 'examples' ? ' is-drop-active' : ''}`}
              data-image-drop="examples"
              aria-label="样图投放区"
              onDragEnter={(event) => onDrop(event, 'examples')}
              onDragOver={(event) => onDrop(event, 'examples')}
              onDrop={(event) => onDrop(event, 'examples')}
            >
              {!brief.templateId && (sources.length ? (
                <>
                  <div className="if-examples">
                    {sources.map((asset, index) => (
                      <div className="if-example" key={asset.id}>
                        <AssetImage path={asset.path} name={asset.name} />
                        <small>第 {index + 1} 页</small>
                        <IconButton label={`移除样图 ${index + 1}`} disabled={busy}
                          onClick={() => onChange({ workflowInput: { mode: 'replace', sources: sources.filter((item) => item.id !== asset.id) } })}>
                          <X size={12} />
                        </IconButton>
                      </div>
                    ))}
                  </div>
                  <div className="if-drop-bar">
                    <small>{dropActive && dropTarget === 'examples' ? '松开即可继续导入' : '还可以按页继续添加'}</small>
                    <Button size="sm" disabled={busy} onClick={onImportExamples}><ImagePlus size={16} />添加样图</Button>
                  </div>
                </>
              ) : (
                <div className="if-replace-upload">
                  <span className="if-upload-icon"><Layers3 size={25} strokeWidth={1.5} /></span>
                  <div className="if-upload-copy">
                    <strong>{dropActive && dropTarget === 'examples' ? '松开即可导入样图' : '拖入样图，或按页选择'}</strong>
                    <small>PNG、JPG、WebP</small>
                  </div>
                  <div className="if-upload-actions">
                    <Button className="if-upload-button" disabled={busy} onClick={onImportExamples}>选择现成套图</Button>
                  </div>
                </div>
              ))}
              <p className="if-replace-label">已有换货模板</p>
              <TemplateSearchList choices={choices} value={brief.templateId || ''} busy={busy}
                onChange={updateTemplate} listLabel="换货模板" emptyLabel={brief.templateId ? '改回上传样图' : undefined} />
            </div>
          )}
          {brief.feature === 'smart' && (
            <TemplateSearchList choices={choices} value={brief.templateId || ''} busy={busy} onChange={updateTemplate} />
          )}
        </section>
      )}
      <section className="if-materials" data-image-drop="products" onDragEnter={(event) => onDrop(event, 'products')} onDragOver={(event) => onDrop(event, 'products')} onDrop={(event) => onDrop(event, 'products')}>
        <div className="if-section-title">
          <h3>{quick ? '参考图片' : replace ? '要换进去的商品' : '商品素材'}</h3>
          {quick && <span>可选</span>}
        </div>
        <div className={`if-upload is-upload-area${brief.products.length ? ' is-upload-area--filled' : ''}${dropActive && dropTarget !== 'examples' ? ' is-drop-active' : ''}`} aria-label="商品素材投放区">
          {!brief.products.length ? (
            <>
              <span className="if-upload-icon"><ImagePlus size={25} strokeWidth={1.5} /></span>
              <div className="if-upload-copy"><strong>{dropActive ? '松开即可导入' : quick ? '拖入参考图片' : '拖入商品图片或文件夹'}</strong><small>PNG、JPG、WebP</small></div>
              <div className="if-upload-actions">
                <Button className="if-upload-button" disabled={busy} onClick={() => onImport(batch)}>{batch ? '选择商品文件夹' : '选择图片'}</Button>
                {!quick && !batch && <Button variant="ghost" size="sm" disabled={busy} onClick={() => onImport(true)}><FolderOpen size={14} />导入文件夹</Button>}
              </div>
            </>
          ) : brief.products.map((product) => (
            <div className="if-product" key={product.id}>
              {(brief.products.length > 1 || (product.name && product.name !== '商品素材')) && <strong>{product.name}</strong>}
              <div className="if-assets">
                {product.assets.filter((asset) => !asset.name.startsWith('__dsimage_')).map((asset) => (
                  <div className="if-asset" key={asset.id}>
                    <AssetImage path={asset.path} name={asset.name} />
                    <IconButton label={`移除素材 ${asset.name}`} disabled={busy} onClick={() => removeAsset(product.id, asset)}><X size={12} /></IconButton>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
        {!!brief.products.length && (
          <div className="if-drop-bar">
            <small>{dropActive && dropTarget !== 'examples' ? '松开即可继续导入' : '还可以把图片继续拖进来'}</small>
            <Button size="sm" disabled={busy} onClick={() => onImport(batch)}><Plus size={14} />添加素材</Button>
          </div>
        )}
      </section>
      <section className="if-requirements">
        <Field label="图片要求">
          <textarea className="kv-textarea custom-scrollbar" disabled={busy} value={brief.requirement}
            onChange={(event) => onChange({ requirement: event.target.value })}
            placeholder={quick ? '例如：把背景换成浅色木桌，保留商品原样，不要文字。' : replace ? '有哪些额外要求？不填则保留原版式，只替换商品。' : brief.feature === 'smart' ? '有哪些额外要求？不填则沿用模板。' : batch ? '例如：这批商品用于店铺上新，每款做一套主图、卖点和场景图。' : '例如：设计一套简洁的电商主图，突出材质和容量，使用中文。'}
            rows={3} />
        </Field>
      </section>
      <div className="if-options">
        <Field label={quick ? '张数' : '每款张数'}><input className="kv-input" type="number" min={1} max={30} disabled={busy || !!template || (replace && !!sources.length)}
          value={pages}
          onChange={(e) => onChange({ count: Math.max(1, Math.min(30, Number(e.target.value) || 1)) })} /></Field>
        <Field label="比例"><ImageRatioSelect disabled={busy} model={model} protocol={protocol} ratio={brief.ratio} resolution={brief.resolution}
          onChange={(output) => onChange(output)} /></Field>
        <Field label="分辨率"><ImageResolutionSelect disabled={busy} model={model} protocol={protocol} ratio={brief.ratio} resolution={brief.resolution}
          onChange={(output) => onChange(output)} /></Field>
        <Field label="图内文字"><ImageLanguageSelect disabled={busy} allowFollowExample={replace} inheritedValue={template?.data.language} value={brief.language} onChange={(language) => onChange({ language })} /></Field>
      </div>
      <details className="if-more">
        <summary>更多设置</summary>
        <Field label="统一风格（可选）"><textarea className="kv-textarea custom-scrollbar" rows={2} disabled={busy} value={brief.style} onChange={(e) => onChange({ style: e.target.value })} placeholder="有特别的色调、背景或排版要求，可以在这里补充" /></Field>
        {brief.products.map((product) => <Field key={product.id} label={brief.products.length > 1 ? `${product.name} 的商品信息（可选）` : '商品信息（可选）'}>
          <textarea className="kv-textarea custom-scrollbar" rows={2} disabled={busy} value={product.facts} placeholder="可补充图片看不出的信息，例如尺寸、容量"
            onChange={(e) => onChange({ products: brief.products.map((item) => item.id === product.id ? { ...item, facts: e.target.value } : item) })} />
        </Field>)}
      </details>
      <div className="if-submit">
        <p>{issue || nextStep}</p>
        <Button variant="primary" disabled={busy || !!issue} onClick={onStart}>
          {busy ? <Loader2 size={16} className="is-spinning" /> : null}{START_LABELS[brief.feature]}<ArrowRight size={15} />
        </Button>
      </div>
    </div>
  )
}
