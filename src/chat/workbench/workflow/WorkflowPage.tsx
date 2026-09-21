import { useCallback, useEffect, useRef, useState } from 'react'
import { Plus, RefreshCw, Upload } from 'lucide-react'
import { Button, IconButton } from '../../../components/Button'
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

  const load = useCallback(() => {
    setItems(workflowStore.list())
  }, [])

  useEffect(() => {
    load()
  }, [load])

  const createBlank = useCallback(() => {
    openFlow(workflowStore.create(t.workbenchWorkflowsUntitled).id)
  }, [t])

  const cloneTemplate = useCallback((id: string) => {
    const template = officialTemplates(t).find((item) => item.id === id)
    if (!template) return
    openFlow(workflowStore.save(template.build()).id)
  }, [t])

  const importFile = useCallback(async (file: File) => {
    setError('')
    try {
      const parsed = parseWorkflow(JSON.parse(await file.text()))
      if (!parsed) {
        setError(t.wfImportBad)
        return
      }
      openFlow(workflowStore.save({ ...parsed, id: crypto.randomUUID(), createdAt: new Date().toISOString() }).id)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
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
    <WorkbenchPage
      title={t.workbenchNavWorkflows}
      actions={(
        <div className="workbench-page-actions">
          <input
            className="workbench-search workbench-search--narrow"
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t.workbenchWorkflowsSearch}
          />
          <IconButton label={t.workbenchRefresh} size="sm" onClick={load}>
            <RefreshCw size={14} />
          </IconButton>
          <Button size="sm" variant="ghost" onClick={() => fileRef.current?.click()}>
            <Upload size={14} />
            {t.wfImport}
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
          <Button size="sm" variant="primary" onClick={createBlank}>
            <Plus size={14} />
            {t.workbenchWorkflowsNew}
          </Button>
        </div>
      )}
    >
      {error ? <p className="workbench-inline-note">{error}</p> : null}
      <div className="workbench-split">
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
                      <h3 className="workbench-flow-name">{item.name || t.workbenchWorkflowsUntitled}</h3>
                      <p className="workbench-page-sub">
                        {t.workbenchWorkflowsNodes
                          .replace('{nodes}', String(item.nodes.length))
                          .replace('{edges}', String(item.edges.length))}
                      </p>
                    </div>
                    <Button size="sm" variant="primary" onClick={() => openFlow(item.id)}>
                      {t.workbenchWorkflowsOpen}
                    </Button>
                  </div>
                  <dl className="workbench-flow-meta">
                    <div>
                      <dt>{t.workbenchWorkflowsUpdated}</dt>
                      <dd>{formatStamp(item.updatedAt)}</dd>
                    </div>
                    <div>
                      <dt />
                      <dd>
                        <button type="button" className="workbench-tab" onClick={() => exportFlow(item)}>
                          {t.wfExport}
                        </button>
                      </dd>
                    </div>
                  </dl>
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
              <button
                key={item.id}
                type="button"
                className="workbench-tpl-card"
                onClick={() => cloneTemplate(item.id)}
              >
                <span className="workbench-tpl-name">{item.name}</span>
                <span className="workbench-page-sub">{item.hint}</span>
                <span className="workbench-page-sub">
                  {t.workbenchWorkflowsNodes
                    .replace('{nodes}', String(item.nodes))
                    .replace('{edges}', String(item.edges))}
                </span>
              </button>
            ))}
          </div>
        </WorkbenchCard>
      </div>
    </WorkbenchPage>
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
