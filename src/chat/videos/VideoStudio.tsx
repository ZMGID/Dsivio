import { useEffect, useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { Clapperboard, History, Plus } from 'lucide-react'
import { api } from '../../api/tauri'
import { Button } from '../../components/Button'
import type { MediaOutput } from '../../generated/mediaGeneration'
import { MediaGenerationRunner } from '../workbench/MediaGenerationRunner'
import { WorkbenchMediaModelSelect } from '../workbench/WorkbenchMediaModelSelect'
import { VideoImageUpload } from './VideoImageUpload'
import '../images/imageStudio.css'
import '../images/studioLayout.css'
import '../studio/executionStatus.css'
import './VideoStudio.css'

/** Preserve the Dsivio studio view; execution belongs to the shared media runner. */
export default function VideoStudio() {
  const [view, setView] = useState<'creation' | 'results'>('creation')
  const [history, setHistory] = useState<MediaOutput[]>([])
  const [error, setError] = useState('')
  useEffect(() => {
    let active = true
    void api.legacyVideoOutputs().then(items => { if (active) setHistory(items) }).catch(failure => { if (active) setError(String(failure)) })
    return () => { active = false }
  }, [])
  return <WorkbenchMediaModelSelect kind="videoModels" render={(modelControl, provider, model) =>
    <MediaGenerationRunner provider={provider} model={model} kind="video" onSubmitted={() => setView('results')} renderImages={VideoImageUpload}
      render={generation => <div className="kv image-studio video-studio">
        <div className="is-shell">
          <aside className="is-rail">
            <div className="is-rail-label">视频工作台</div>
            <nav aria-label="视频工作台">
              <Button variant="ghost" className={view === 'creation' ? 'active' : ''} aria-current={view === 'creation' ? 'page' : undefined} onClick={() => setView('creation')}><Clapperboard size={17} /><span>视频创作</span></Button>
              <Button variant="ghost" className={view === 'results' ? 'active' : ''} aria-current={view === 'results' ? 'page' : undefined} onClick={() => setView('results')}><History size={17} /><span>生成与成片</span></Button>
            </nav>
            <div className="is-rail-history custom-scrollbar">
              <div className="is-rail-label">最近任务</div>
              {!generation.tasks.length && <p>生成任务和成片会保存在这里。</p>}
              {generation.tasks.slice(0, 6).map(task => <Button key={task.id} variant="ghost" title={`${task.model} · ${new Date(task.createdAt).toLocaleString()}`} onClick={() => setView('results')}>
                <span className={`is-history-dot ${task.status}`} /><span>{new Date(task.createdAt).toLocaleString()}</span>
              </Button>)}
            </div>
          </aside>
          <div className="studio-workspace">
            <main className="is-main custom-scrollbar">
              <div className="vs-heading"><h2>{view === 'creation' ? '视频创作' : '生成与成片'}</h2><Button size="sm" disabled={generation.busy} onClick={() => { generation.reset(); setView('creation') }}><Plus size={14} />新建</Button></div>
              <div className="is-work-toolbar"><div className="is-stage-tabs" role="tablist" aria-label="视频制作步骤">
                <Button variant="ghost" role="tab" aria-selected={view === 'creation'} onClick={() => setView('creation')}><span>1</span>素材与要求</Button>
                <Button variant="ghost" role="tab" aria-selected={view === 'results'} onClick={() => setView('results')}><span>2</span>生成与成片</Button>
              </div></div>
              <div hidden={view !== 'creation'}>
                <fieldset disabled={generation.busy} className="vs-layout min-w-0">
                  <div className="vs-editor-column">
                    {generation.assets && <section className="vs-panel vs-media">{generation.assets}</section>}
                    <section className="vs-panel">{generation.prompt}</section>
                  </div>
                  <section className="vs-panel vs-options"><div className="flex min-w-0 flex-col gap-4">{modelControl}{generation.options}</div></section>
                  <div className="vs-actions studio-primary-actions">{generation.action}</div>
                </fieldset>
              </div>
              <div hidden={view !== 'results'}>
                {generation.results}
                {history.length > 0 && <section className="vs-panel"><h3>历史作品</h3><div className="flex min-w-0 flex-col gap-4">{history.map(output => <article key={output.path} className="vs-result">
                  <div className="vs-result-preview"><video className="vs-video" src={convertFileSrc(output.path)} controls preload="metadata" /></div>
                  <div className="vs-result-info"><p className="[overflow-wrap:anywhere]">{output.path.split(/[\\/]/).pop()}</p><div className="vs-actions"><Button size="sm" onClick={() => void api.openLocalFile(output.path).catch(failure => setError(String(failure)))}>打开文件</Button></div></div>
                </article>)}</div></section>}
              </div>
              {(generation.error || error) && <p role="alert" className="vs-muted">{generation.error || error}</p>}
            </main>
          </div>
        </div>
      </div>} />
  } />
}
