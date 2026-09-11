import { useEffect, useState, type DragEvent } from 'react'
import type { ImageDropZone } from './studioDrop'
import { open } from '@tauri-apps/plugin-dialog'
import {
  ArrowDown,
  ArrowUp,
  Check,
  Download,
  FileImage,
  FolderOpen,
  Image as ImageIcon,
  Layers3,
  Loader2,
  Plus,
  Save,
  Sparkles,
  X,
} from 'lucide-react'
import { api } from '../../api/tauri'
import { ImageRatioSelect, ImageResolutionSelect } from './ImageOutputSelect'
import { humanizeImageError } from './imageValidation'
import { Button, IconButton } from '../../components/Button'
import { AssetImage, Field, ImageLanguageSelect, StudioSelect } from './StudioPanels'
import {
  latestResults,
  suggestImageTaskName,
  workflowInputsChanged,
  workflowSamplesComplete,
  type ImageAction,
  type ImageBrief,
  type ImageProduct,
  type ImageResult,
  type ImageTask,
  type ImageTemplate,
} from './types'
import './imageWorkflow.css'

type Props = {
  configurationIssue?: string
  model?: string
  protocol?: string
  brief: ImageBrief
  task: ImageTask | null
  busy: boolean
  draftSaved: boolean
  onChange: (patch: Partial<ImageBrief>) => void
  onAction: (action: ImageAction) => Promise<void>
  perform: (fn: () => Promise<void>) => Promise<void>
  onNew: () => void
  onOpenResult: (result: ImageResult) => void
  onExport: () => void
  dropActive?: boolean
  dropTarget?: ImageDropZone | null
  onDropSurface?: (event: DragEvent, zone?: ImageDropZone) => void
}

export function ImageWorkflow({
  configurationIssue,
  model,
  protocol,
  brief,
  task,
  busy,
  draftSaved,
  onChange,
  onAction,
  perform,
  onNew,
  onOpenResult,
  onExport,
  dropActive = false,
  dropTarget = null,
  onDropSurface,
}: Props) {
  const input = brief.workflowInput || { mode: 'smart' as const, sources: [] }
  const workflow = task?.workflow
  const template = task?.templates[0]
  const rulesChanged = !!task && workflowInputsChanged(brief, task.brief)
  const current = !!workflow?.rulesCurrent && !rulesChanged
  const dirty = !task || JSON.stringify(brief) !== JSON.stringify(task.brief)
  const sampleInputsChanged = !!workflow?.sampleIds.some(
    (id) =>
      JSON.stringify(brief.products.find((p) => p.id === id)) !==
      JSON.stringify(task?.brief.products.find((p) => p.id === id)),
  )
  const approved =
    current && !sampleInputsChanged && workflow?.approvedVersion === workflow?.ruleVersion
  const [feedback, setFeedback] = useState('')
  const [showHistory, setShowHistory] = useState(false)
  const sampleIds = (
    (workflow?.approvedVersion === workflow?.ruleVersion && workflow?.sampleIds.length ? workflow.sampleIds : brief.products.slice(0, 2).map((p) => p.id))
  ).filter((id) => brief.products.some((p) => p.id === id))
  const results = task ? latestResults(task) : []
  const selectedSamplesMatch =
    !!workflow && [...sampleIds].sort().join(',') === [...workflow.sampleIds].sort().join(',')
  const complete = !!task && workflowSamplesComplete(task) && selectedSamplesMatch
  const unresolved = results.some(
    (r) => (r.remoteId || r.downloadUrl) && !r.path && !r.error?.startsWith('远程图片任务失败'),
  )
  const successful = results.filter((r) => r.path)
  const productionPending = template
    ? brief.products.reduce(
        (n, p) =>
          n +
          template.data.slots.filter(
            (slot) =>
              JSON.stringify(p) !==
                JSON.stringify(task?.brief.products.find((saved) => saved.id === p.id)) ||
              !results.some((r) => r.productId === p.id && r.slotId === slot.id),
          ).length,
        0,
      )
    : 0
  const history =
    task?.results.filter(
      (r, i, all) =>
        !results.some((currentResult) => currentResult.id === r.id) &&
        r.path &&
        !all.slice(i + 1).some((next) => next.path === r.path),
    ) || []

  useEffect(() => {
    setFeedback('')
  }, [workflow?.ruleVersion])

  const patchProduct = (id: string, patch: Partial<ImageProduct>) =>
    onChange({
      products: brief.products.map((p) => (p.id === id ? { ...p, ...patch } : p)),
    })
  const importImages = (target: 'source' | 'product' | 'folder', productId?: string) =>
    perform(async () => {
      const paths = await open(
        target === 'folder'
          ? { directory: true, multiple: true, title: '每个商品放一个文件夹，可一次选择多款' }
          : {
              multiple: true,
              title:
                target === 'source'
                  ? input.mode === 'replace'
                    ? '按页序选择现成套图'
                    : '选择商品照片'
                  : '选择同一款商品的正面、背面和细节图',
              filters: [{ name: '图片', extensions: ['png', 'jpg', 'jpeg', 'webp'] }],
            },
      )
      if (!paths) return
      const products = await api.imageStudioImport(
        Array.isArray(paths) ? paths : [paths],
        target === 'folder',
      )
      const assets = products.flatMap((p) => p.assets)
      if (target === 'source') {
        if (input.sources.length + assets.length > 30) throw new Error('原始参考素材最多 30 张')
        onChange({ workflowInput: { ...input, sources: [...input.sources, ...assets] } })
      } else if (productId) {
        const product = brief.products.find((p) => p.id === productId)
        if (!product) return
        if (product.assets.length + assets.length > 16) throw new Error('每款商品最多 16 张参考图')
        patchProduct(productId, {
          assets: [...product.assets, ...assets],
          front: product.front || assets[0]?.id || null,
        })
      } else {
        if (products.some((p) => p.assets.length > 16))
          throw new Error('每款商品最多 16 张参考图，请按商品分别选择')
        if (brief.products.length + products.length > 200)
          throw new Error('一个任务最多 200 款商品')
        onChange({ products: [...brief.products, ...products] })
      }
    })
  const moveSource = (index: number, delta: number) => {
    const sources = [...input.sources]
    ;[sources[index], sources[index + delta]] = [sources[index + delta], sources[index]]
    onChange({ workflowInput: { ...input, sources } })
  }
  const step = approved ? 3 : !current ? 0 : results.length ? 2 : 1
  const fromSet = input.mode === 'replace'

  return (
    <div className="iw-workflow">
      {configurationIssue && <p role="status">{configurationIssue}</p>}
      <div className="is-work-heading">
        <div>
          <h2>制作模板</h2>
          <p>把现成套图或设计要求做成模板，下次换商品继续用。</p>
        </div>
        <div className="is-actions">
          <Button size="sm" variant="ghost" disabled={busy} onClick={onNew}>
            新流程
          </Button>
          <span className="if-draft-status">{task && !dirty ? '已保存' : draftSaved ? '草稿保存在本机' : '草稿保存失败'}</span>
        </div>
      </div>
      <ol className="iw-progress" aria-label="制作模板进度">
        {['提供素材', '试做效果', '调整模板', '完成复用'].map((label, index) => (
          <li
            key={label}
            aria-current={step === index ? 'step' : undefined}
            className={step >= index ? 'done' : ''}
          >
            <span>{index < step ? <Check size={11} /> : index + 1}</span>
            {label}
          </li>
        ))}
      </ol>
      {task?.error && (
        <p role="alert" className="is-alert is-error">
          {humanizeImageError(task.error)}
        </p>
      )}
      {rulesChanged && <p className="iw-hint">制作素材或要求已修改，请先更新制作，再进行试品。</p>}

      <details className="iw-panel iw-source" open={!template || !current}>
        <summary>
          <strong>01 · 原始素材</strong>
          <span>
            {input.sources.length
              ? `${input.sources.length} 张 · ${fromSet ? '按现成套图换品' : '从商品照片设计'}`
              : fromSet
                ? '按现成套图换品'
                : '从商品照片设计'}
          </span>
        </summary>
        <div className="iw-panel-body">
          <div className="iw-start" role="radiogroup" aria-label="参考图类型">
            <label className={input.mode === 'smart' ? 'selected' : ''}>
              <input
                type="radio"
                name="workflow-start"
                aria-label="商品照片"
                checked={input.mode === 'smart'}
                disabled={busy}
                onChange={() => onChange({ workflowInput: { ...input, mode: 'smart' }, language: brief.language === '跟随样图' ? 'zh-CN' : brief.language })}
              />
              <ImageIcon size={16} strokeWidth={1.6} />
              <strong>商品照片</strong>
              <b>新做一套</b>
            </label>
            <label className={fromSet ? 'selected' : ''}>
              <input
                type="radio"
                name="workflow-start"
                aria-label="现成套图"
                checked={fromSet}
                disabled={busy}
                onChange={() => onChange({ workflowInput: { ...input, mode: 'replace' }, language: brief.language === 'zh-CN' ? '跟随样图' : brief.language })}
              />
              <Layers3 size={16} strokeWidth={1.6} />
              <strong>现成套图</strong>
              <b>只换商品</b>
            </label>
          </div>
          <div className="is-brief-layout">
            <div className="is-editor-column">
              <div
                className={`is-upload-area${input.sources.length ? ' is-upload-area--filled' : ''}${dropActive && dropTarget !== 'products' ? ' is-drop-active' : ''}`}
                data-image-drop="sources"
                aria-label="原始参考投放区"
                onDragEnter={(event) => onDropSurface?.(event, 'sources')}
                onDragOver={(event) => onDropSurface?.(event, 'sources')}
                onDrop={(event) => onDropSurface?.(event, 'sources')}
              >
                {input.sources.length ? (
                  <>
                    <div className="iw-assets">
                      {input.sources.map((asset, i) => (
                        <div className="iw-asset" key={asset.id}>
                          <AssetImage path={asset.path} name={asset.name} />
                          <span title={asset.name}>
                            {i + 1}. {asset.name}
                          </span>
                          <div>
                            <IconButton
                              label={`前移参考图 ${i + 1}`}
                              disabled={busy || i === 0}
                              onClick={() => moveSource(i, -1)}
                            >
                              <ArrowUp size={12} />
                            </IconButton>
                            <IconButton
                              label={`后移参考图 ${i + 1}`}
                              disabled={busy || i === input.sources.length - 1}
                              onClick={() => moveSource(i, 1)}
                            >
                              <ArrowDown size={12} />
                            </IconButton>
                            <IconButton
                              label={`移除参考图 ${i + 1}`}
                              disabled={busy}
                              onClick={() =>
                                onChange({
                                  workflowInput: {
                                    ...input,
                                    sources: input.sources.filter((a) => a.id !== asset.id),
                                  },
                                })
                              }
                            >
                              <X size={12} />
                            </IconButton>
                          </div>
                        </div>
                      ))}
                    </div>
                  </>
                ) : (
                  <>
                    <div className="is-upload-illustration">
                      <FileImage size={35} strokeWidth={1.1} />
                      <span>
                        <Plus size={14} />
                      </span>
                    </div>
                    <h3>
                      {dropActive && dropTarget !== 'products'
                        ? '松开即可导入'
                        : fromSet
                          ? '按页序把套图拖到这里'
                          : '把商品照片拖到这里'}
                    </h3>
                    <p>
                      {fromSet
                        ? '第一张是第 1 页，可调顺序'
                        : 'PNG / JPG / WebP'}
                    </p>
                    <Button disabled={busy} onClick={() => void importImages('source')}>
                      {fromSet ? '选择现成套图' : '选择商品照片'}
                    </Button>
                  </>
                )}
              </div>
              {!!input.sources.length && (
                <div className="iw-drop-bar">
                  <small>{dropActive && dropTarget !== 'products' ? '松开即可继续导入' : '还可以把图片继续拖进来'}</small>
                  <Button size="sm" disabled={busy} onClick={() => void importImages('source')}>
                    <Plus size={14} />
                    {fromSet ? '继续添加套图' : '继续添加照片'}
                  </Button>
                </div>
              )}
              <Field label="制作要求">
                <textarea className="kv-textarea custom-scrollbar" rows={3} disabled={busy}
                  value={brief.requirement} onChange={(event) => onChange({ requirement: event.target.value })}
                  placeholder={fromSet ? '例如：保留这套图的版式，以后只换商品。' : '例如：做一套简洁的电商主图模板，包含卖点、细节和场景。'} />
              </Field>
            </div>
            <aside className="is-spec-column">
              <section className="is-section is-spec-section">
                <details className="if-more"><summary>更多设置</summary><Field label="任务名称">
                  <input
                    className="kv-input"
                    disabled={busy}
                    value={brief.name}
                    onChange={(e) => onChange({ name: e.target.value })}
                    placeholder={suggestImageTaskName(brief)}
                  />
                </Field>
                <Field label="使用平台">
                  <StudioSelect
                    disabled={busy}
                    value={brief.platform}
                    onChange={(e) => onChange({ platform: e.target.value })}
                  >
                    {['通用电商', 'Amazon', 'Mercado Livre', 'Shopee', 'TikTok Shop', '独立站'].map(
                      (v) => (
                        <option key={v}>{v}</option>
                      ),
                    )}
                  </StudioSelect>
                </Field>
                </details><Field label="图内语言">
                  <ImageLanguageSelect
                    allowFollowExample={fromSet}
                    disabled={busy}
                    value={brief.language}
                    onChange={(language) => onChange({ language })}
                  />
                </Field>
                <Field label="比例">
                  <ImageRatioSelect
                    disabled={busy}
                    model={model}
                    protocol={protocol}
                    ratio={brief.ratio}
                    resolution={brief.resolution}
                    onChange={(output) => onChange(output)}
                  />
                </Field>
                <Field label="分辨率">
                  <ImageResolutionSelect
                    disabled={busy}
                    model={model}
                    protocol={protocol}
                    ratio={brief.ratio}
                    resolution={brief.resolution}
                    onChange={(output) => onChange(output)}
                  />
                </Field>
                <Field
                  label="每套页数"
                  hint={fromSet ? '一张套图对应一页，可在投放区调整顺序。' : undefined}
                >
                  <input
                    className="kv-input"
                    type="number"
                    min={1}
                    max={30}
                    disabled={busy || fromSet}
                    value={fromSet ? input.sources.length : brief.count}
                    onChange={(e) =>
                      onChange({ count: Math.max(1, Math.min(30, Number(e.target.value) || 1)) })
                    }
                  />
                </Field>
              </section>
            </aside>
            <div className="is-creation-footer">
              <div>
                <small>
                  {!input.sources.length
                    ? fromSet
                      ? '先把现成套图按页拖进来或点选'
                      : '先把商品照片拖进来或点选'
                    : !brief.requirement.trim()
                      ? '写清这次要做成什么样'
                      : template
                        ? '素材或要求变了，会按新内容重做规则'
                        : '开始后会生成一套可换品复用的规则'}
                </small>
              </div>
              <Button
                variant="primary"
                disabled={busy || !!configurationIssue || !input.sources.length || !brief.requirement.trim() || unresolved}
                onClick={() => void onAction({ kind: 'workflow_build' })}
              >
                <Sparkles size={15} />
                {template ? '更新模板' : '制作模板'}
              </Button>
            </div>
          </div>
        </div>
      </details>

      {template && (
        <section className="iw-panel" aria-label="共用规则">
          <div className="iw-panel-title">
            <h3>已制作 · {template.data.name}</h3>
            <span className="iw-version">
              v{workflow?.ruleVersion}
              {approved ? ' · 已确认' : ' · 待试品确认'}
            </span>
          </div>
          <p className="iw-hint">{workflow?.changes.at(-1)?.summary}</p>
          <div className="iw-slot-list">
            {template.data.slots.map((slot) => (
              <span key={slot.id}>
                {slot.id} · {slot.purpose || '商品图'}
              </span>
            ))}
          </div>
          <details>
            <summary>查看与编辑共用规则</summary>
            <RulesEditor
              key={`${task?.id}-${workflow?.ruleVersion}`}
              template={template}
              busy={busy || !current || unresolved}
              onAction={onAction}
            />
          </details>
        </section>
      )}

      {template && <section className="iw-panel" aria-label="换品试做">
        <div className="iw-panel-title">
          <div>
            <h3>02 · 换其他商品试做</h3>
            <p className="iw-hint">
              添加商品图片，系统自动挑选最多两款试做。
            </p>
          </div>
        </div>
        <div
          className={`iw-product-drop${brief.products.length ? ' is-upload-area--filled' : ' is-upload-area'}${dropActive && dropTarget === 'products' ? ' is-drop-active' : ''}`}
          data-image-drop="products"
          aria-label="试做商品投放区"
          onDragEnter={(event) => onDropSurface?.(event, 'products')}
          onDragOver={(event) => onDropSurface?.(event, 'products')}
          onDrop={(event) => onDropSurface?.(event, 'products')}
        >
        {!brief.products.length ? (
          <>
            <div className="is-upload-illustration">
              <ImageIcon size={35} strokeWidth={1.1} />
              <span>
                <Plus size={14} />
              </span>
            </div>
            <h3>{dropActive && dropTarget === 'products' ? '松开即可导入' : '把商品图片或文件夹拖到这里'}</h3>
            <p>规则制作完成后，在这里添加其他商品来验证效果。</p>
            <div className="iw-drop-actions">
              <Button size="sm" disabled={busy} onClick={() => void importImages('product')}>
                <Plus size={14} />
                添加一款商品
              </Button>
              <Button size="sm" variant="ghost" disabled={busy} onClick={() => void importImages('folder')}>
                <FolderOpen size={14} />
                按文件夹添加多款
              </Button>
            </div>
          </>
        ) : (
        <div className="iw-products">
          {brief.products.map((product) => (
            <article className="iw-product" key={product.id}>
              <div className="iw-product-top">
                <span className="iw-hint">{sampleIds.includes(product.id) ? '自动试做' : '确认效果后生成'}</span>
                <IconButton
                  label={`移除商品 ${product.name}`}
                  disabled={busy}
                  onClick={() =>
                    onChange({ products: brief.products.filter((p) => p.id !== product.id) })
                  }
                >
                  <X size={14} />
                </IconButton>
              </div>
              <div className="iw-product-cover">
                <AssetImage
                  path={
                    product.assets.find((a) => a.id === product.front)?.path ||
                    product.assets[0]?.path ||
                    ''
                  }
                  name={product.name}
                />
              </div>
              <Field label="商品名称">
                <input
                  className="kv-input"
                  disabled={busy}
                  value={product.name}
                  onChange={(e) => patchProduct(product.id, { name: e.target.value })}
                />
              </Field>
              <details>
                <summary>素材与商品信息 · {product.assets.length} 张</summary>
                <div className="iw-product-assets">
                  {product.assets.filter((asset) => !asset.name.startsWith('__dsimage_')).map((asset) => (
                    <div key={asset.id}>
                      <AssetImage path={asset.path} name={asset.name} />
                      <span title={asset.name}>{asset.name}</span>
                      <IconButton
                        label={`移除素材 ${asset.name}`}
                        disabled={busy}
                        onClick={() =>
                          patchProduct(product.id, {
                            assets: product.assets.filter((a) => a.id !== asset.id),
                            front: product.front === asset.id ? null : product.front,
                            back: product.back === asset.id ? null : product.back,
                          })
                        }
                      >
                        <X size={12} />
                      </IconButton>
                    </div>
                  ))}
                </div>
                <Button
                  size="sm"
                  disabled={busy}
                  onClick={() => void importImages('product', product.id)}
                >
                  补充商品图片
                </Button>
                <Field label="这款商品的真实信息">
                  <textarea
                    className="kv-textarea custom-scrollbar"
                    rows={3}
                    disabled={busy}
                    value={product.facts}
                    onChange={(e) => patchProduct(product.id, { facts: e.target.value })}
                    placeholder="已确认的尺寸、容量、材质和卖点；不确定的留空"
                  />
                </Field>
              </details>
            </article>
          ))}
        </div>
        )}
        </div>
        {!!brief.products.length && (
          <div className="iw-drop-bar">
            <small>{dropActive && dropTarget === 'products' ? '松开即可继续导入' : '还可以把商品图片或文件夹继续拖进来'}</small>
            <div className="iw-drop-actions">
              <Button size="sm" disabled={busy} onClick={() => void importImages('product')}>
                <Plus size={14} />
                添加一款商品
              </Button>
              <Button size="sm" variant="ghost" disabled={busy} onClick={() => void importImages('folder')}>
                <FolderOpen size={14} />
                按文件夹添加多款
              </Button>
            </div>
          </div>
        )}
        <div className="iw-panel-title iw-trial-action">
          <span className="iw-hint">
            {sampleIds.length ? `先试做 ${sampleIds.length} 款商品` : '添加商品后即可试做'}
            {template ? ` · 每款 ${template.data.slots.length} 页` : ''}
          </span>
          <Button
            variant="primary"
            disabled={busy || !!configurationIssue || !current || !sampleIds.length}
            onClick={() => void onAction({ kind: 'workflow_trial', sampleIds })}
          >
            <Sparkles size={14} />
            试做模板效果
          </Button>
        </div>
      </section>}

      {template && (
        <section className="iw-panel" aria-label="试品结果与反馈">
          <div className="iw-panel-title">
            <h3>03 · 对照结果，反馈修正</h3>
            <span className="iw-hint">本版已生成 {successful.length} 张</span>
          </div>
          <p className="iw-hint">
            对整套效果的反馈会更新共用规则，并自动重新生成已选试品。以后添加的商品沿用修改后的规则。
          </p>
          {!results.length && (
            <p className="iw-empty">试品生成后，原始参考和结果会在这里并排展示。</p>
          )}
          {brief.products
            .filter((p) => task?.plans.some((plan) => plan.productId === p.id))
            .map((product) => (
              <div className="iw-result-product" key={product.id}>
                <h4>
                  {product.name}
                  {workflow?.sampleIds.includes(product.id) ? ' · 试品' : ''}
                </h4>
                <div className="iw-results">
                  {template.data.slots.map((slot, index) => {
                    const result = results.find(
                      (r) => r.productId === product.id && r.slotId === slot.id,
                    )
                    const source = input.sources[input.mode === 'replace' ? index : 0]
                    const remote =
                      (result?.remoteId || result?.downloadUrl) &&
                      !result.path &&
                      !result.error?.startsWith('远程图片任务失败')
                    return (
                      <article className="iw-result" key={slot.id}>
                        <strong>
                          {slot.id} · {slot.purpose || '商品图'}
                        </strong>
                        <div className="iw-compare">
                          <figure>
                            {source && (
                              <AssetImage path={source.path} name={`原始参考 ${index + 1}`} />
                            )}
                            <figcaption>原始参考</figcaption>
                          </figure>
                          <figure>
                            {result?.path ? (
                              <button
                                type="button"
                                className="iw-image-button"
                                aria-label={`查看 ${product.name} ${slot.id}`}
                                onClick={() => onOpenResult(result)}
                              >
                                <AssetImage
                                  path={result.path}
                                  name={`${product.name} ${slot.id} 生成结果`}
                                />
                              </button>
                            ) : (
                              <div className="iw-wait">
                                {busy ? <Loader2 size={20} className="is-spinning" /> : null}
                                {result?.downloadUrl ? '等待恢复下载' : remote ? '远程生成中' : result?.error ? '本页未完成' : '等待生成'}
                              </div>
                            )}
                            <figcaption>当前结果 · v{workflow?.ruleVersion}</figcaption>
                          </figure>
                        </div>
                        {result?.error && <p className="iw-result-error">{humanizeImageError(result.error)}</p>}
                        <div className="is-actions">
                          {result?.path ? (
                            <Button size="sm" onClick={() => onOpenResult(result)}>
                              查看 / 只改这张
                            </Button>
                          ) : (
                            result && (
                              <Button
                                size="sm"
                                disabled={busy || dirty || !current}
                                onClick={() =>
                                  void onAction(
                                    remote
                                      ? { kind: 'resume', resultId: result.id }
                                      : { kind: 'retry', productId: product.id, slotId: slot.id },
                                  )
                                }
                              >
                                {result.downloadUrl ? '恢复下载' : remote ? '恢复查询' : '重试本页'}
                              </Button>
                            )
                          )}
                          <Button
                            size="sm"
                            disabled={busy || !current}
                            onClick={() =>
                              setFeedback(
                                (text) =>
                                  `${text}${text ? '\n' : ''}${product.name} 的 ${slot.id}（${slot.purpose || '商品图'}）：`,
                              )
                            }
                          >
                            修改这页模板
                          </Button>
                        </div>
                      </article>
                    )
                  })}
                </div>
              </div>
            ))}
          <Field label="模板要怎么改">
            <textarea
              className="kv-textarea custom-scrollbar"
              rows={4}
              disabled={busy || !current}
              value={feedback}
              onChange={(e) => setFeedback(e.target.value)}
              placeholder="例如：h2 产品太小，以后所有商品在这一页都放大到画面宽度的 70%；h3 不要沿用原商品的容量，改成读取每款商品的真实信息。"
            />
          </Field>
          <Button
            variant="primary"
            disabled={busy || !!configurationIssue || !current || !feedback.trim() || unresolved}
            onClick={() => void onAction({ kind: 'workflow_refine', note: feedback })}
          >
            <Sparkles size={14} />
            修改模板并重新试做
          </Button>
          {unresolved && (
            <p className="iw-hint">先恢复查询未完成的远程图片，再修改规则，以保留本次生成结果。</p>
          )}
        </section>
      )}

      {template && (
        <section
          className={`iw-panel iw-production ${approved ? 'confirmed' : ''}`}
          aria-label="持续出图"
        >
          <div>
            <h3>04 · {approved ? `沿用 v${workflow?.ruleVersion} 持续出图` : '确认试品效果'}</h3>
            <p className="iw-hint">
              {approved
                ? `继续在上方添加商品即可使用这版规则。待生成 ${productionPending} 页，已有页面会保留。`
                : '可以先检查试品，也可以直接用当前规则生成其他商品。'}
            </p>
          </div>
          <div className="is-actions">
            {!approved && current && <Button disabled={busy || !!configurationIssue || dirty || !brief.products.length || !productionPending} onClick={() => void onAction({ kind: 'workflow_produce' })}>直接生成新增商品</Button>}
            {approved ? (
              <Button
                variant="primary"
                disabled={busy || !!configurationIssue || !brief.products.length || !productionPending}
                onClick={() => void onAction({ kind: 'workflow_produce' })}
              >
                <Sparkles size={14} />
                生成新增商品
              </Button>
            ) : (
              <Button
                variant="primary"
                disabled={busy || dirty || !current || !complete}
                onClick={() => void onAction({ kind: 'workflow_approve' })}
              >
                <Check size={14} />
                试品满意，确认这版
              </Button>
            )}
            <Button disabled={busy || dirty || !successful.length} onClick={onExport}>
              <Download size={14} />
              导出本版图片
            </Button>
          </div>
        </section>
      )}

      {!!workflow?.changes.length && (
        <details className="iw-panel iw-history">
          <summary>
            <strong>修改记录与历史图片</strong>
            <span>{workflow.changes.length} 个规则版本</span>
          </summary>
          <div className="iw-panel-body">
            {[...workflow.changes].reverse().map((change) => (
              <article key={change.version}>
                <strong>
                  v{change.version} · {new Date(change.createdAt).toLocaleString()}
                </strong>
                <p>{change.note}</p>
                <p className="iw-hint">{change.summary}</p>
                <details>
                  <summary>查看当时的共用规则</summary>
                  <p>{change.template.data.style}</p>
                  {change.template.data.slots.map((slot) => (
                    <p key={slot.id}>
                      {slot.id} · {slot.purpose}：{slot.brief || slot.prompt}
                    </p>
                  ))}
                </details>
              </article>
            ))}
            <Button size="sm" onClick={() => setShowHistory(!showHistory)}>
              {showHistory ? '收起历史图片' : `查看历史图片（${history.length} 张）`}
            </Button>
            {showHistory && (
              <div className="iw-history-images">
                {history.map((result) => (
                  <button
                    type="button"
                    key={result.id}
                    className="iw-image-button"
                    onClick={() => onOpenResult(result)}
                  >
                    <AssetImage path={result.path!} name={result.slotId} />
                    <span>
                      {brief.products.find((p) => p.id === result.productId)?.name || '历史商品'} ·{' '}
                      {result.slotId} · v
                      {[...workflow.changes].reverse().find((c) => c.revision <= result.revision)
                        ?.version || 1}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </div>
        </details>
      )}
    </div>
  )
}

function RulesEditor({
  template,
  busy,
  onAction,
}: {
  template: ImageTemplate
  busy: boolean
  onAction: Props['onAction']
}) {
  const [data, setData] = useState(template.data)
  const changed = JSON.stringify(data) !== JSON.stringify(template.data)
  const slotKey = data.mode === 'replace' ? 'prompt' : 'brief'
  return (
    <div className="iw-rules-editor">
      <Field label="统一风格">
        <textarea
          className="kv-textarea custom-scrollbar"
          disabled={busy}
          rows={4}
          value={data.style || ''}
          onChange={(e) => setData({ ...data, style: e.target.value })}
        />
      </Field>
      <Field label="共用文案规则">
        <textarea
          className="kv-textarea custom-scrollbar"
          disabled={busy}
          rows={3}
          value={String(data.text_policy || '')}
          onChange={(e) => setData({ ...data, text_policy: e.target.value })}
        />
      </Field>
      {data.slots.map((slot, index) => (
        <Field key={slot.id} label={`${slot.id} · ${slot.purpose || '页面规则'}`}>
          <textarea
            className="kv-textarea custom-scrollbar"
            disabled={busy}
            rows={4}
            value={slot[slotKey] || ''}
            onChange={(e) =>
              setData({
                ...data,
                slots: data.slots.map((s, i) =>
                  i === index ? { ...s, [slotKey]: e.target.value } : s,
                ),
              })
            }
          />
        </Field>
      ))}
      <Button
        disabled={busy || !changed}
        onClick={() =>
          void onAction({ kind: 'workflow_edit', templateData: data, note: '手动修改共用规则' })
        }
      >
        <Save size={14} />
        保存规则并重试
      </Button>
    </div>
  )
}
