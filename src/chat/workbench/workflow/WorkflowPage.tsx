import { useCallback, useEffect, useRef, useState } from 'react'
import { Plus, RefreshCw, Upload, Download, Copy, Trash2, ArrowRight } from 'lucide-react'
import { Button, IconButton } from '../../../components/Button'
import { Input } from '../../../settings/public/controls'
import { useT } from '../../../components/i18n'
import { setHash } from '../../chatRoutes'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { workbenchWorkflowHash, workbenchWorkflowIdFromHash } from '../workbenchPages'
import { officialTemplates } from './workflowCatalog'
import { WorkflowCanvas } from './WorkflowCanvas'
import { parseWorkflow, workflowStore } from './workflowStore'
import type { GenerationWorkflow } from './workflowModel'

function formatStamp(iso: string): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  const mm = String(date.getMonth() + 1).padStart(2, '0')
  const dd = String(date.getDate()).padStart(2, '0')
  const hh = String(date.getHours()).padStart(2, '0')
  const mi = String(date.getMinutes()).padStart(2, '0')
  return `${mm}/${dd} ${hh}:${mi}`
}

function openFlow(id: string): void {
  setHash(workbenchWorkflowHash(id))
}

function WorkflowList() {
  const t = useT()
  const fileRef = useRef<HTMLInputElement>(null)
  const [items, setItems] = useState<GenerationWorkflow[]>(() => workflowStore.list())
  const [query, setQuery] = useState('')
  const [error, setError] = useState('')
  const [removed, setRemoved] = useState<GenerationWorkflow | null>(null)
  const [importing, setImporting] = useState(false)
  const importLock = useRef(false)

  const load = useCallback(() => {
    setItems(workflowStore.list())
  }, [])

  useEffect(() => {
    load()
  }, [load])

  const createBlank = useCallback(() => {
    try { openFlow(workflowStore.create(t.workbenchWorkflowsUntitled).id) }
    catch (failure) { setError(`保存失败：${String(failure)}`) }
  }, [t])

  const cloneTemplate = useCallback((id: string) => {
    const template = officialTemplates(t).find((item) => item.id === id)
    if (!template) return
    try { openFlow(workflowStore.save(template.build()).id) }
    catch (failure) { setError(`保存失败：${String(failure)}`) }
  }, [t])

  const importFile = useCallback(async (file: File) => {
    if (importLock.current) return
    importLock.current = true; setImporting(true); setError('')
    try {
      const parsed = parseWorkflow(JSON.parse(await file.text()))
      if (!parsed) {
        setError(t.wfImportBad)
        return
      }
      openFlow(workflowStore.save({ ...parsed, id: crypto.randomUUID(), createdAt: new Date().toISOString() }).id)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally { importLock.current = false; setImporting(false) }
  }, [t])

  const exportFlow = useCallback((flow: GenerationWorkflow) => {
    const blob = new Blob([JSON.stringify(flow, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const link = document.createElement('a')
    link.href = url
    link.download = `${flow.name || 'workflow'}.json`
    link.click()
    URL.revokeObjectURL(url)
  }, [])

  const visible = items.filter((item) => item.name.toLowerCase().includes(query.trim().toLowerCase()))
  const templates = officialTemplates(t)

  return (
    <div className="workbench-workflows-page">
    <WorkbenchPage
      title={t.workbenchNavWorkflows}
      actions={(
        <>
          <div className="workbench-workflows-search"><Input
            aria-label={t.workbenchWorkflowsSearch}
            type="search"
            value={query}
            onChange={setQuery}
            placeholder={t.workbenchWorkflowsSearch}
          /></div>
          <IconButton label={t.workbenchRefresh} size="sm" onClick={load}>
            <RefreshCw size={14} />
          </IconButton>
          <Button disabled={importing} onClick={() => fileRef.current?.click()}>
            <Upload size={14} />
            {importing ? '正在导入…' : t.wfImport}
          </Button>
          <input
            ref={fileRef}
            type="file"
            accept="application/json"
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0]
              event.target.value = ''
              if (file) void importFile(file)
            }}
          />
          <Button variant="primary" onClick={createBlank}>
            <Plus size={14} />
            {t.workbenchWorkflowsNew}
          </Button>
        </>
      )}
    >
      {error ? <p role="alert" className="workbench-inline-note">{error}</p> : null}
      {removed && <p className="workbench-inline-note">已删除“{removed.name}”<Button size="sm" onClick={() => { try { workflowStore.save(removed); setRemoved(null); load() } catch (failure) { setError(String(failure)) } }}>撤销删除</Button></p>}
      <div className="workbench-workflows-columns">
        <WorkbenchCard
          title={t.workbenchWorkflowsMine}
          extra={<span className="workbench-page-sub">{t.workbenchWorkflowsMineCount.replace('{n}', String(visible.length))}</span>}
        >
          {visible.length === 0 ? (
            <WorkbenchEmpty>{t.workbenchWorkflowsEmptyHint}</WorkbenchEmpty>
          ) : (
            <div className="workbench-flow-grid">
              {visible.map((item) => (
                <article key={item.id} className="workbench-flow-card">
                  <div className="workbench-flow-card-top">
                    <div className="min-w-0">
                      <h3 className="workbench-flow-name" title={item.name}>{item.name || t.workbenchWorkflowsUntitled}</h3>
                      <p className="workbench-page-sub">
                        {t.workbenchWorkflowsNodes
                          .replace('{nodes}', String(item.nodes.length))
                          .replace('{edges}', String(item.edges.length))}
                      </p>
                    </div>
                    <Button onClick={() => openFlow(item.id)}>
                      {t.workbenchWorkflowsOpen}<ArrowRight size={14} />
                    </Button>
                  </div>
                  <p className="workbench-flow-updated">{t.workbenchWorkflowsUpdated} · {formatStamp(item.updatedAt)}</p>
                  <div className="workbench-flow-card-actions" role="group" aria-label={`${item.name}的操作`}>
                    <Button onClick={() => exportFlow(item)}><Download size={14} />{t.wfExport}</Button>
                    <Button onClick={() => { try { workflowStore.save({ ...structuredClone(item), id: crypto.randomUUID(), name: `${item.name} 副本`, createdAt: new Date().toISOString() }); load() } catch (failure) { setError(String(failure)) } }}><Copy size={14} />复制</Button>
                    <Button onClick={() => { try { workflowStore.remove(item.id); setRemoved(item); load() } catch (failure) { setError(String(failure)) } }}><Trash2 size={14} />删除</Button>
                  </div>
                </article>
              ))}
            </div>
          )}
        </WorkbenchCard>

        <WorkbenchCard
          title={t.workbenchWorkflowsTemplates}
          extra={<span className="workbench-page-sub">{t.workbenchWorkflowsTemplateHint}</span>}
        >
          <div className="workbench-tpl-list">
            {templates.map((item) => (
              <article key={item.id} className="workbench-tpl-card">
                <h3 className="workbench-tpl-name">{item.name}</h3>
                <p className="workbench-page-sub">{item.hint}</p>
                <div className="workbench-tpl-footer">
                  <span className="workbench-page-sub">{t.workbenchWorkflowsNodes.replace('{nodes}', String(item.nodes)).replace('{edges}', String(item.edges))}</span>
                  <Button aria-label={`使用${item.name}模板`} onClick={() => cloneTemplate(item.id)}>使用模板<ArrowRight size={14} /></Button>
                </div>
              </article>
            ))}
          </div>
        </WorkbenchCard>
      </div>
      <p className="workbench-page-sub">导入导出仅包含配置，本机素材需单独保留。</p>
    </WorkbenchPage>
    </div>
  )
}

/**
 * 工作台工作流：生成管线列表 + 画布。
 * 和桌面自动化不是同一套图，不读写 automationApi。
 */
export function WorkflowPage() {
  const [flow, setFlow] = useState<GenerationWorkflow | null>(() => {
    const id = workbenchWorkflowIdFromHash()
    return id ? workflowStore.get(id) : null
  })

  useEffect(() => {
    const sync = () => {
      const id = workbenchWorkflowIdFromHash()
      setFlow(id ? workflowStore.get(id) : null)
    }
    window.addEventListener('hashchange', sync)
    return () => window.removeEventListener('hashchange', sync)
  }, [])

  if (flow) return <WorkflowCanvas key={flow.id} flow={flow} />
  return <WorkflowList />
}
