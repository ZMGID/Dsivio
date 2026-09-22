import type { DragEvent } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { Clapperboard, FileImage, Film, FolderOpen, Plus, WandSparkles, X } from 'lucide-react'
import { Button, IconButton } from '../../../../components/Button'
import { Select } from '../../../../settings/public/controls'
import { Field, StudioSelect } from '../../image/projects/StudioPanels'
import { RequirementComposer } from '../../image/projects/RequirementComposer'
import { WorkbenchMediaModelSelect } from '../../WorkbenchMediaModelSelect'
import { mediaModelKey } from '../../../../data/mediaModelPools'
import { videoModel } from '../../../../data/videoModels'
import { videoRatios, languages, type VideoBrief, type VideoBootstrap } from './types'
import { VideoMediaOptions } from './VideoMediaOptions'
import type { VideoDropZone } from './videoDrop'
import { AssetImage } from './VideoAssetImage'
export interface VideoFormProps {
 brief: VideoBrief
 data: VideoBootstrap
 busy: string
 native: boolean
 dropActive: boolean
 dropTarget: VideoDropZone | null
 change: (patch: Partial<VideoBrief>) => void
 keepOsDrop: (event: DragEvent, zone?: VideoDropZone) => void
 guarded: (label: string, fn: (current: () => boolean) => Promise<void>) => Promise<void>
 pickImages: () => Promise<void>
 setError: (value: string) => void
 inputIssue: string
 resolutions: string[]
 run: (action: string) => Promise<void>
}

export function VideoSourceFields({ brief, native, dropActive, dropTarget, change, keepOsDrop, guarded }: VideoFormProps) { return (
                        <>
                          <div
                            className={`vs-drop${brief.source ? ' is-upload-area--filled' : ''}${dropActive && dropTarget !== 'images' ? ' is-drop-active' : ''}`}
                            data-video-drop="source"
                            aria-label="参考视频投放区"
                            onDragEnter={(event) => keepOsDrop(event, 'source')}
                            onDragOver={(event) => keepOsDrop(event, 'source')}
                            onDrop={(event) => keepOsDrop(event, 'source')}
                          >
                            {brief.source ? (
                              <div className="vs-drop-file">
                                <span className="vs-drop-mark">
                                  <Film size={20} strokeWidth={1.6} />
                                </span>
                                <div>
                                  <strong>
                                    {brief.source.split(/[\\/]/).pop() || brief.source}
                                  </strong>
                                  <span title={brief.source}>
                                    {dropActive && dropTarget !== 'images'
                                      ? '松开即可替换'
                                      : '还可以把视频继续拖进来替换'}
                                  </span>
                                </div>
                                <IconButton
                                  label="移除参考视频"
                                  onClick={() => change({ source: '' })}
                                >
                                  <X size={14} />
                                </IconButton>
                              </div>
                            ) : (
                              <div className="vs-drop-empty">
                                <span className="vs-drop-mark">
                                  <Film size={22} strokeWidth={1.5} />
                                </span>
                                <strong>
                                  {dropActive && dropTarget !== 'images'
                                    ? '松开即可导入'
                                    : '把参考视频拖到这里'}
                                </strong>
                                <span>MP4 / MOV / WebM</span>
                              </div>
                            )}
                          </div>
                          <div className="vs-drop-bar">
                            <input
                              className="kv-input"
                              aria-label="视频链接或本地路径"
                              placeholder="粘贴视频链接，或把视频拖到这里"
                              value={brief.source}
                              onChange={(e) =>
                                change({ source: e.target.value })
                              }
                            />
                            <Button
                              size="sm"
                              disabled={!native}
                              onClick={() =>
                                void guarded('选择视频…', async current => {
                                  const path = await open({
                                    filters: [
                                      {
                                        name: '视频',
                                        extensions: ['mp4', 'mov', 'webm'],
                                      },
                                    ],
                                  })
                                  if (typeof path === 'string' && current())
                                    change({ source: path })
                                })
                              }
                            >
                              <FolderOpen size={14} />
                              {brief.source ? '更换本地视频' : '选择本地视频'}
                            </Button>
                          </div>
                          <Field label="分析方式">
                            <StudioSelect
                              value={brief.analysisMethod || 'auto'}
                              onChange={(event) =>
                                change({ analysisMethod: event.target.value as VideoBrief['analysisMethod'] })
                              }
                            >
                              <option value="auto">自动：有视频模型则直传，否则使用 MCP</option>
                              <option value="model">视频模型</option>
                              <option value="mcp">video-analyzer MCP</option>
                            </StudioSelect>
                          </Field>
                        </>
) }

export function VideoProductFields({ brief, native, dropActive, dropTarget, change, keepOsDrop, pickImages }: VideoFormProps) { return (
                        <>
                          <div
                            className={`vs-drop${brief.images.length ? ' is-upload-area--filled' : ''}${dropActive && dropTarget !== 'referenceVideos' && dropTarget !== 'referenceAudios' ? ' is-drop-active' : ''}`}
                            data-video-drop="images"
                            aria-label="商品素材投放区"
                            onDragEnter={(event) => keepOsDrop(event, 'images')}
                            onDragOver={(event) => keepOsDrop(event, 'images')}
                            onDrop={(event) => keepOsDrop(event, 'images')}
                          >
                            {brief.images.length ? (
                              <div className="vs-assets">
                                {brief.images.map((path) => (
                                  <div key={path}>
                                    <AssetImage
                                      path={path}
                                      name={path.split(/[\\/]/).pop() || '参考图'}
                                    />
                                    <IconButton
                                      label="移除参考图"
                                      onClick={() =>
                                        change({
                                          images: brief.images.filter(
                                            (p) => p !== path,
                                          ),
                                        })
                                      }
                                    >
                                      <X size={13} />
                                    </IconButton>
                                  </div>
                                ))}
                              </div>
                            ) : (
                              <div className="vs-drop-empty">
                                <span className="vs-drop-mark">
                                  <FileImage size={22} strokeWidth={1.5} />
                                </span>
                                <strong>
                                  {dropActive && dropTarget !== 'referenceVideos' && dropTarget !== 'referenceAudios'
                                    ? '松开即可导入'
                                    : '把商品图片拖到这里'}
                                </strong>
                                <span>PNG / JPG / WebP</span>
                              </div>
                            )}
                          </div>
                          <div className="vs-drop-bar">
                            <small>
                              {dropActive
                                ? '松开即可继续导入'
                                : brief.images.length
                                  ? '还可以把图片继续拖进来'
                                  : '图片用途由系统自动匹配'}
                            </small>
                            <Button
                              size="sm"
                              disabled={!native}
                              onClick={() => void pickImages()}
                            >
                              <Plus size={14} />
                              {brief.images.length ? '继续添加参考图' : '选择商品图片'}
                            </Button>
                          </div>
                        </>
) }

export function RemakeProductFields({ brief, native, dropActive, dropTarget, change, keepOsDrop, pickImages }: VideoFormProps) { return (                    <section className="vs-panel">
                      <h3>我的商品</h3>
                      <div
                        className={`vs-drop${brief.images.length ? ' is-upload-area--filled' : ''}${dropActive && dropTarget === 'images' ? ' is-drop-active' : ''}`}
                        data-video-drop="images"
                        aria-label="商品图片投放区"
                        onDragEnter={(event) => keepOsDrop(event, 'images')}
                        onDragOver={(event) => keepOsDrop(event, 'images')}
                        onDrop={(event) => keepOsDrop(event, 'images')}
                      >
                        {brief.images.length ? (
                          <div className="vs-images">{brief.images.map(path => <div key={path}><AssetImage path={path} name={path.split(/[\\/]/).pop() || '商品'} /><IconButton label="移除商品图片" onClick={() => change({ images: brief.images.filter(p => p !== path) })}><X size={14} /></IconButton></div>)}</div>
                        ) : (
                          <div className="vs-drop-empty">
                            <span className="vs-drop-mark">
                              <FileImage size={22} strokeWidth={1.5} />
                            </span>
                            <strong>{dropActive && dropTarget === 'images' ? '松开即可导入' : '把商品图片拖到这里'}</strong>
                            <span>PNG / JPG / WebP</span>
                          </div>
                        )}
                      </div>
                      <div className="vs-drop-bar">
                        <small>
                          {dropActive && dropTarget === 'images'
                            ? '松开即可继续导入'
                            : brief.images.length
                              ? '还可以把商品图片继续拖进来'
                              : '参考视频定镜头，图片定商品外观'}
                        </small>
                        <Button size="sm" disabled={!native} onClick={() => void pickImages()}>
                          <Plus size={14} />
                          {brief.images.length ? '继续添加商品图片' : '添加商品图片'}
                        </Button>
                      </div>
                    </section>) }

export function VideoBriefLayout({ media, extraMedia, isAnalysis = false, remake = false, brief, data, busy, native, dropActive, dropTarget, change, keepOsDrop, setError, inputIssue, resolutions, run }: VideoFormProps & { media: import('react').ReactNode; extraMedia?: import('react').ReactNode; isAnalysis?: boolean; remake?: boolean }) {
 const route = brief.route
 return (
                <div className="vs-layout">
                  <div className="vs-editor-column">
                    <section
                      className="vs-panel vs-media"
                      onDragEnter={(event) => keepOsDrop(event, isAnalysis ? 'source' : 'images')}
                      onDragOver={(event) => keepOsDrop(event, isAnalysis ? 'source' : 'images')}
                      onDrop={(event) => keepOsDrop(event, isAnalysis ? 'source' : 'images')}
                    >
                      <div className="vs-media-head">
                        <h3>{isAnalysis ? '参考视频' : '商品素材'}</h3>
                        <small>
                          {isAnalysis
                            ? '单条视频 · 拆解不会提交生成'
                            : '也可以只写要求，不放图'}
                        </small>
                      </div>
                      {media}

                    </section>
                    {extraMedia}
                    <section className="vs-panel">
                      {isAnalysis ? <Field label="重点分析什么（可选）"><textarea className="kv-textarea vs-request custom-scrollbar"
                        value={brief.request} onChange={e => change({ request: e.target.value })}
                        placeholder="例如：重点看开场、商品展示和镜头节奏。留空则完整拆解。" /></Field> : <RequirementComposer label="这次要拍什么"
                        value={brief.request}
                        onChange={request => change({ request, selectedConcept: undefined })}
                        placeholder="例如：让背包在自然光下缓慢转动，展示正面细节，不要口播。" />}
                      <label className="flex min-w-0 flex-col gap-2"><span>选择视频模板</span><Select
                        value={brief.template?.id || ''}
                        disabled={!!busy}
                        onChange={id => change({ template: data.templates.find(t => t.id === id) })}
                        options={[{ value: '', label: '不使用模板' }, ...data.templates.map(t => ({ value: t.id, label: `${t.name} · ${t.kind === 'reference' ? '参考' : '生成'}` }))]}
                      /></label>
                      {brief.template && (
                        <div className="vs-notice">
                          已选模板：{brief.template.name}
                          <IconButton
                            label="取消模板"
                            onClick={() => change({ template: undefined })}
                          >
                            <X size={14} />
                          </IconButton>
                        </div>
                      )}
                    </section>
                    {!isAnalysis && (
                      <details className="vs-panel vs-advanced">
                        <summary>声音与高级参考设置</summary>
                        <VideoMediaOptions onError={setError}
                          disabled={false}
                          brief={brief}
                          change={change}
                          native={native}
                          dropActive={dropActive}
                          dropTarget={dropTarget}
                          onDrop={keepOsDrop}

                        />
                      </details>
                    )}
                  </div>
                  <section className="vs-panel vs-options">
                    {!isAnalysis && <Field label="声音">
                      <StudioSelect value={brief.speechMode || 'auto'} onChange={e => change({ speechMode: e.target.value as VideoBrief['speechMode'] })}>
                        <option value="auto">按要求自动设计</option><option value="ambient">环境音与音乐</option><option value="dialogue">口播 / 对白</option><option value="silent">静音</option>
                      </StudioSelect>
                    </Field>}
                    {(isAnalysis || brief.speechMode === 'dialogue') && <Field label={isAnalysis ? "报告语言" : "口播 / 文案语言"}>
                      <StudioSelect
                        value={
                          languages.some(([v]) => v === brief.language)
                            ? brief.language
                            : 'custom'
                        }
                        onChange={(e) => change({ language: e.target.value })}
                      >
                        {languages.map(([v, name]) => (
                          <option key={v} value={v}>
                            {name}
                          </option>
                        ))}
                      </StudioSelect>
                    </Field>}
                    {(!languages.some(([v]) => v === brief.language) ||
                      brief.language === 'custom') && (
                      <Field label="其他语言">
                        <input
                          className="kv-input"
                          value={
                            brief.language === 'custom' ? '' : brief.language
                          }
                          onChange={(e) => change({ language: e.target.value })}
                          placeholder="输入语言名称或地区代码"
                        />
                      </Field>
                    )}
                    {!isAnalysis && (
                      <>
                        <Field label="视频时长（秒）">
                          <input
                            className="kv-input"
                            type="number"
                            min={1}
                            max={videoModel(brief.model || '')?.durations.length ? Math.max(...videoModel(brief.model || '')!.durations) : undefined}
                            value={brief.duration}
                            onChange={(e) =>
                              change({ duration: Number(e.target.value) })
                            }
                          />
                        </Field>
                        <Field label="画幅">
                          <StudioSelect
                            value={brief.ratio}
                            onChange={(e) => change({ ratio: e.target.value })}
                          >
                            {(route
                              ? (videoModel(brief.model || '')?.ratios || videoRatios[route])
                              : videoRatios.comfy
                            ).map((v) => (
                              <option key={v} value={v}>
                                {v === 'adaptive' ? '自适应' : v}
                              </option>
                            ))}
                          </StudioSelect>
                        </Field>
                        <WorkbenchMediaModelSelect kind="videoModels" value={brief.providerId && brief.model ? mediaModelKey({ providerId: brief.providerId, model: brief.model }) : ''}
                          onChange={(providerId, model, provider) => {
                            const profile = videoModel(model)
                            const protocol = provider?.modelOverrides?.[model]?.videoProtocol || profile?.protocol
                            change({ providerId, model, route: provider?.request.comfy ? 'comfy' : protocol === 'xai_video' ? 'grok' : protocol === 'minimax_h3' ? 'minimax' : 'native',
                              inputMode: 'auto', resolution: String(profile?.defaults.resolution ?? ''), ratio: String(profile?.defaults.ratio ?? '9:16'), duration: Number(profile?.defaults.duration ?? 10) })
                          }} render={control => control} />
                        <Field
                          label={
                            '生成清晰度'
                          }
                        >
                          <StudioSelect
                            disabled={!route}
                            value={brief.resolution}
                            onChange={(e) =>
                              change({ resolution: e.target.value })
                            }
                          >
                            <option value="">请选择</option>
                            {resolutions.map((v) => (
                              <option key={v} value={v}>
                                {route === 'comfy' ? (v === '0.5' ? '标准' : '高清') : v}
                              </option>
                            ))}
                          </StudioSelect>
                        </Field>
                      </>
                    )}
                  </section>
                  {inputIssue && <p className="vs-muted" role="status">{inputIssue} <a href="#chat/settings/media">配置模型池</a></p>}
                  <div className="vs-actions studio-primary-actions">
                    {!isAnalysis && <Button
                      disabled={!native || !!busy || !brief.request.trim()}
                      onClick={() => void run('use_prompt')}
                      title="原文作为生成提示词，跳过方案设计和改写，进入生成页"
                    >
                      <Clapperboard size={15} />使用原提示词生成
                    </Button>}
                    <Button
                      variant="primary"
                      disabled={
                        !native ||
                        !!busy ||
                        (isAnalysis
                          ? !brief.source.trim() || (remake && !brief.images.length)
                          : !brief.request.trim() && !brief.template && !brief.images.length)
                      }
                      onClick={() =>
                        void run(isAnalysis ? 'analyze' : 'plan')
                      }
                    >
                      <WandSparkles size={15} />
                      {remake ? '分析参考并适配商品' : isAnalysis ? '开始拆解' : '帮我设计视频'}
                    </Button>

                  </div>
                </div>
)
}
