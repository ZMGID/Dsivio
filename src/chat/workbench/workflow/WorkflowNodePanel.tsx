import { useEffect, useState } from 'react'
import { isTauriRuntime, type ModelProvider, type Settings } from '../../../api/tauri'
import { getSettingsCached, subscribeSettings } from '../../../api/settingsCache'
import { Button } from '../../../components/Button'
import { FieldBlock, Input, Select, TextArea } from '../../../settings/public/controls'
import { mediaModelKey } from '../../../data/mediaModelPools'
import { resolveModelInfo } from '../../../data/modelMatching'
import { videoModel } from '../../../data/videoModels'
import { listImageOutputs, imageOutputKey } from '../image/projects/imageOutput'
import { WorkbenchMediaModelSelect } from '../WorkbenchMediaModelSelect'
import { nodeConfig, type WorkflowConfig, type WorkflowModelChoice } from './workflowConfig'
import { inputSource } from './workflowGraph'
import { paletteEntry } from './workflowCatalog'
import { WorkflowAssets } from './WorkflowAssets'
import type { GenerationWorkflow, WorkflowNode } from './workflowModel'

function UnderstandingModel({ value, onChange, vision = true }: { value: WorkflowModelChoice | null; vision?: boolean; onChange: (value: WorkflowModelChoice | null) => void }) {
  const [settings, setSettings] = useState<Settings | null>(null), [error, setError] = useState(''), [reload, setReload] = useState(0)
  useEffect(() => {
    if (!isTauriRuntime()) { setError('请在桌面应用中读取和选择本机模型。'); return }
    let active = true, updated = false
    const stop = subscribeSettings(value => { updated = true; if (active) { setSettings(value); setError('') } })
    void getSettingsCached().then(value => { if (active && !updated) { setSettings(value); setError('') } }).catch(e => { if (active && !updated) setError(String(e)) })
    return () => { active = false; stop() }
  }, [reload])
  const entries = settings?.providers.filter(p => p.enabled !== false && !p.request.comfy).flatMap(p => p.enabledModels.filter(model => (vision ? resolveModelInfo(model, p.modelOverrides, p).capabilities?.vision : !resolveModelInfo(model, p.modelOverrides, p).capabilities?.imageGeneration && !resolveModelInfo(model, p.modelOverrides, p).capabilities?.videoGeneration)).map(model => ({ providerId: p.id, model, label: `${p.name} / ${model}`, value: mediaModelKey({ providerId: p.id, model }) }))) ?? []
  const selected = value ? mediaModelKey(value) : ''
  return <FieldBlock label={vision ? "理解模型" : "文本模型"}>
    <Select ariaLabel={vision ? "理解模型" : "文本模型"} value={selected} options={[{ value: '', label: vision ? '选择支持图片理解的模型' : '选择文本模型' }, ...entries, ...(selected && !entries.some(e => e.value === selected) ? [{ value: selected, label: `${value?.model}（不可用，请重选）` }] : [])]} onChange={key => { const entry = entries.find(e => e.value === key); onChange(entry ? { providerId: entry.providerId, model: entry.model } : null) }} />
    {settings && !entries.length && <p>{vision ? '请在设置中启用支持视觉的模型。' : '请在设置中启用文本模型。'}</p>}
    {error && <div role="alert">模型加载失败：{error}<Button size="sm" onClick={() => setReload(n => n + 1)}>重试</Button></div>}
  </FieldBlock>
}

function GenerationOptions({ node, config, provider, model, onChange }: { node: WorkflowNode; config: Extract<WorkflowConfig, { type: 'generate' }>; provider?: ModelProvider; model: string; onChange: (config: WorkflowConfig) => void }) {
  if (!provider || !model) return null
  const set = (key: string, value: string) => onChange({ ...config, options: { ...config.options, [key]: value } })
  const select = (key: string, label: string, choices: (string | number)[]) => <FieldBlock key={key} label={label}><Select ariaLabel={label} value={config.options[key] ?? ''} options={[{ value: '', label: '模型默认值' }, ...choices.map(v => ({ value: String(v), label: String(v) })), ...(config.options[key] && !choices.map(String).includes(config.options[key]) ? [{ value: config.options[key], label: `${config.options[key]}（不可用，请重选）` }] : [])]} onChange={v => set(key, v)} /></FieldBlock>
  const comfy = provider.request.comfy?.workflows.find(w => w.id === model)
  if (comfy) return <>{comfy.inputs.map(binding => {
    if (binding.source?.type === 'prompt' || binding.source?.type === 'image') return <FieldBlock key={`${binding.nodeId}:${binding.input}`} label={binding.label}><p className="workbench-page-sub">使用节点的{binding.source.type === 'prompt' ? '提示词' : '参考图片'}。</p></FieldBlock>
    const key = `${binding.nodeId}:${binding.input}`
    const value = config.options[key] ?? String(comfy.graph[binding.nodeId]?.inputs[binding.input] ?? '')
    return <FieldBlock key={key} label={binding.label}>{binding.kind === 'image'
      ? <p className="workbench-page-sub">使用节点的参考图片。</p>
      : <Input aria-label={binding.label} type={binding.kind === 'number' ? 'number' : 'text'} value={value} onChange={value => set(key, value)} />}</FieldBlock>
  })}</>
  if (node.kind === 'image.generate') {
    const choices = listImageOutputs(model)
    return <FieldBlock label="图片比例与尺寸"><Select ariaLabel="图片比例与尺寸" value={config.options.output ?? ''} options={[{ value: '', label: '模型默认值' }, ...choices.map(item => ({ value: imageOutputKey(item.ratio, item.resolution), label: `${item.ratio} · ${item.resolution.toUpperCase()}` }))]} onChange={value => set('output', value)} /></FieldBlock>
  }
  const profile = videoModel(model)
  return profile ? <>
    <FieldBlock label="图片用途"><Select ariaLabel="图片用途" value={config.options.imageMode ?? 'firstFrame'} options={[{ value: 'firstFrame', label: '视频首帧' }, { value: 'reference', label: '参考图片' }]} onChange={value => set('imageMode', value)} /></FieldBlock>
    {select('duration', '时长（秒）', profile.durations)}
    {select('resolution', '分辨率', profile.resolutions)}
    {select('ratio', '比例', profile.ratios)}
    {'audioToggle' in profile && profile.audioToggle && select('audio', '生成声音', ['true', 'false'])}
    {profile.modes.some(mode => mode !== 'text' && mode !== 'image' && mode !== 'reference') && (
      <p className="workbench-page-sub">当前可配置提示词和参考图，其他素材类型暂不支持。</p>
    )}
  </> : <p className="workbench-page-sub">此模型尚无参数定义，保留模型默认参数。</p>
}

export function WorkflowNodePanel({ flow, node, onChange, onClose, onDisconnect }: { flow: GenerationWorkflow; node: WorkflowNode; onChange: (node: WorkflowNode, field: string) => void; onClose: () => void; onDisconnect?: (port: string) => void }) {
  const config = nodeConfig(node)
  const changeConfig = (next: WorkflowConfig) => {
    const previous = config as unknown as Record<string, unknown>
    const fields = Object.entries(next).filter(([key, value]) => JSON.stringify(value) !== JSON.stringify(previous[key])).map(([key]) => key).join(':')
    onChange({ ...node, config: next }, `config:${fields}`)
  }
  const source = (port: string) => inputSource(flow, node.id, port)
  const promptSource = source('prompt'), imageSource = source('image')
  return <aside className="workbench-flow-config custom-scrollbar" aria-label="节点配置">
    <div className="workbench-flow-config-head"><h3>节点配置</h3><Button size="sm" onClick={onClose}>关闭</Button></div>
    <label><span>节点名称</span><Input aria-label="节点名称" value={node.title} onChange={title => onChange({ ...node, title }, 'title')} /></label>
    <label><span>备注</span><TextArea aria-label="备注" value={node.note ?? ''} onChange={note => onChange({ ...node, note }, 'note')} rows={2} /></label>
    {config.type !== 'placeholder' && (paletteEntry(node.kind)?.inputs ?? []).map(port => {
      const upstream = source(port.id)
      return (
        <div className="workbench-flow-source" key={port.id}>
          <strong>{port.label}{port.required ? ' *' : '（可选）'}</strong>
          <span title={upstream ? '断开连接后恢复本地配置' : undefined}>
            {upstream ? `来自：${upstream.title}` : '未连接'}
          </span>
          {upstream && onDisconnect && <Button size="sm" onClick={() => onDisconnect(port.id)}>断开连接</Button>}
        </div>
      )
    })}
    {config.type === 'placeholder' && <p role="status">此节点待完善，当前可编辑名称和备注。</p>}
    {config.type === 'assets' && <WorkflowAssets key={node.id} assets={config.assets} many={node.kind === 'image.uploadMany'} video={node.kind === 'video.upload'} onChange={assets => changeConfig({ ...config, assets })} />}
    {config.type === 'prompt' && <label><span>提示词</span><TextArea aria-label="提示词" value={config.text} onChange={text => changeConfig({ ...config, text })} rows={6} /></label>}
    {config.type === 'text' && <>
      {node.kind !== 'text.join' && <UnderstandingModel vision={false} value={config.model} onChange={model => changeConfig({ ...config, model })} />}
      <label><span>{node.kind === 'text.join' ? '前置文本' : '处理要求'}</span><TextArea aria-label="处理要求" value={config.instruction} onChange={instruction => changeConfig({ ...config, instruction })} rows={4} /></label>
      <fieldset disabled={!!source('text')}><label><span>文本内容</span><TextArea aria-label="文本内容" value={config.text} onChange={text => changeConfig({ ...config, text })} rows={5} /></label></fieldset>
    </>}
    {config.type === 'understand' && <>
      <UnderstandingModel value={config.model} onChange={model => changeConfig({ ...config, model })} />
      <fieldset disabled={!!promptSource}><label><span>分析指令</span><TextArea value={config.instruction} onChange={instruction => changeConfig({ ...config, instruction })} rows={6} /></label></fieldset>
      <WorkflowAssets key={node.id} assets={config.assets} disabled={!!imageSource} onChange={assets => changeConfig({ ...config, assets })} />
    </>}
    {config.type === 'generate' && <>
      <WorkbenchMediaModelSelect kind={node.kind === 'image.generate' ? 'imageModels' : 'videoModels'} value={config.model ? mediaModelKey(config.model) : ''} onChange={(providerId, model) => changeConfig({ ...config, model: providerId ? { providerId, model } : null, options: {} })}
        render={(control, provider, model) => <>{control}<GenerationOptions node={node} config={config} provider={provider} model={model} onChange={changeConfig} /></>} />
      <fieldset disabled={!!promptSource}><label><span>提示词</span><TextArea value={config.prompt} onChange={prompt => changeConfig({ ...config, prompt })} rows={5} /></label></fieldset>
      <WorkflowAssets key={node.id} assets={config.assets} disabled={!!imageSource} onChange={assets => changeConfig({ ...config, assets })} />
    </>}
    {config.type === 'output' && <p className="workbench-page-sub">连接上游节点即可，无需额外配置。</p>}
  </aside>
}
