import { open } from '@tauri-apps/plugin-dialog'
import { Film, Music, Plus, X } from 'lucide-react'
import type { DragEvent } from 'react'
import { Button, IconButton } from '../../components/Button'
import { Field, StudioSelect } from '../images/StudioPanels'
import type { VideoBrief } from './types'
import type { VideoDropZone } from './videoDrop'

export function VideoMediaOptions({
  brief: b,
  change,
  disabled,
  native,
  dropActive = false,
  dropTarget = null,
  onDrop,
  onError,
}: {
  brief: VideoBrief
  change: (v: Partial<VideoBrief>) => void
  disabled: boolean
  native: boolean
  dropActive?: boolean
  dropTarget?: VideoDropZone | null
  onDrop?: (event: DragEvent, zone?: VideoDropZone) => void
  onError: (v: string) => void
}) {
  const mode = b.inputMode || 'auto'
  const reference =
    mode === 'reference' ||
    (mode === 'auto' && (b.images.length > 0 || !!b.voiceIds?.length))
  async function pick(field: 'referenceVideos' | 'referenceAudios') {
    try {
      const result = await open({
        multiple: true,
        filters: [
          {
            name: field === 'referenceVideos' ? '参考视频' : '参考音频',
            extensions:
              field === 'referenceVideos' ? ['mp4', 'mov'] : ['mp3', 'wav'],
          },
        ],
      })
      if (result)
        change({
          [field]: [
            ...new Set([
              ...(b[field] || []),
              ...(Array.isArray(result) ? result : [result]),
            ]),
          ].slice(0, 3),
        })
    } catch (e) {
      onError(String(e))
    }
  }
  return (
    <>
      <Field
        label="生成模式"
        hint="模式决定素材的作用；切换模式不会自动删除已添加的素材。"
      >
        <StudioSelect
          value={mode}
          disabled={disabled || !b.route}
          onChange={(e) =>
            change({
              inputMode: e.target.value as VideoBrief['inputMode'],
            })
          }
        >
          <option value="auto">按素材自动匹配</option>
          <option value="text">文生视频</option>
          {b.route === 'grok' && <option value="image">单图驱动首帧</option>}
          <option value="reference">参考素材生成</option>
          {b.route === 'minimax' && (
            <option value="frames">首帧 / 尾帧控制</option>
          )}
        </StudioSelect>
      </Field>
      {b.route === 'grok' && (
        <p className="vs-muted">
          自动模式将商品图作为外观参考，不锁定首帧。参考生成：最多 7 张，最高
          720p。仅需让原图动起来时选择“单图驱动首帧”，可选 1080p。
        </p>
      )}
      {b.route === 'minimax' && mode === 'frames' && (
        <>
          {(['firstFrame', 'lastFrame'] as const).map((field, i) => (
            <Field key={field} label={i ? '尾帧图片' : '首帧图片'}>
              <StudioSelect
                disabled={disabled}
                value={b[field] || ''}
                onChange={(e) => change({ [field]: e.target.value })}
              >
                <option value="">不指定</option>
                {b.images.map((p, n) => (
                  <option key={p} value={p}>
                    图片 {n + 1} · {p.split(/[\\/]/).pop()}
                  </option>
                ))}
              </StudioSelect>
            </Field>
          ))}
          <p className="vs-muted">
            从商品素材中选择首尾帧；只保留选中的图片，不与参考视频、音频混用。
          </p>
        </>
      )}
      {b.route === 'minimax' && mode !== 'frames' && mode !== 'text' && (
        <>
          {(['referenceVideos', 'referenceAudios'] as const).map((field, i) => (
            <div key={field} className="is-field">
              <span>
                {i ? '参考音频' : '参考视频'} · {(b[field] || []).length} / 3
              </span>
              <div
                className={`vs-drop vs-ref-drop${(b[field] || []).length ? ' is-upload-area--filled' : ''}${dropActive && dropTarget === field ? ' is-drop-active' : ''}`}
                data-video-drop={field}
                aria-label={i ? '参考音频投放区' : '参考视频投放区'}
                onDragEnter={(event) => onDrop?.(event, field)}
                onDragOver={(event) => onDrop?.(event, field)}
                onDrop={(event) => onDrop?.(event, field)}
              >
                {(b[field] || []).length ? (
                  (b[field] || []).map((p, n) => (
                    <div className="vs-drop-file" key={p}>
                      <span className="vs-drop-mark">
                        {i ? <Music size={18} strokeWidth={1.6} /> : <Film size={18} strokeWidth={1.6} />}
                      </span>
                      <div>
                        <strong title={p}>
                          {n + 1}. {p.split(/[\\/]/).pop()}
                        </strong>
                      </div>
                      <IconButton
                        label={i ? '移除参考音频' : '移除参考视频'}
                        disabled={disabled}
                        onClick={() =>
                          change({ [field]: b[field]?.filter((v) => v !== p) })
                        }
                      >
                        <X size={14} />
                      </IconButton>
                    </div>
                  ))
                ) : (
                  <div className="vs-drop-empty">
                    <span className="vs-drop-mark">
                      {i ? <Music size={20} strokeWidth={1.5} /> : <Film size={20} strokeWidth={1.5} />}
                    </span>
                    <strong>
                      {dropActive && dropTarget === field
                        ? '松开即可导入'
                        : i
                          ? '把参考音频拖到这里'
                          : '把参考视频拖到这里'}
                    </strong>
                    <span>{i ? 'MP3 / WAV' : 'MP4 / MOV'}</span>
                  </div>
                )}
              </div>
              <div className="vs-drop-bar">
                <small>
                  {dropActive && dropTarget === field
                    ? '松开即可继续导入'
                    : (b[field] || []).length
                      ? '还可以把文件继续拖进来'
                      : i
                        ? 'MP3 / WAV，最多 3 段'
                        : 'MP4 / MOV，最多 3 段'}
                </small>
                <Button
                  size="sm"
                  disabled={disabled || !native || (b[field]?.length || 0) >= 3}
                  onClick={() => void pick(field)}
                >
                  <Plus size={14} />
                  添加{i ? '音频' : '视频'}
                </Button>
              </div>
            </div>
          ))}
          <p className="vs-muted">
            视频、音频分别最多 3 段，每段 2–15 秒且各自总时长 ≤ 15
            秒。所有参考素材合计最多 12 个。
          </p>
        </>
      )}
      {(b.speechMode || 'dialogue') === 'dialogue' && (
        <Field
          label="口播文案（可选）"
          hint="留空由导演编写；填写后按原文保留。"
        >
          <textarea
            className="kv-textarea custom-scrollbar"
            rows={3}
            disabled={disabled}
            value={b.dialogue || ''}
            onChange={(e) => change({ dialogue: e.target.value })}
          />
        </Field>
      )}
      {b.speechMode !== 'silent' && (
        <Field label="背景音乐要求（可选）">
          <input
            className="kv-input"
            disabled={disabled}
            value={b.music || ''}
            onChange={(e) => change({ music: e.target.value })}
            placeholder="例如：轻快、无配乐、低音量原声"
          />
        </Field>
      )}
      {b.route === 'grok' && reference && b.speechMode !== 'silent' && (
        <Field
          label="Grok 预设音色"
          hint="最多 3 位说话者；音色与口播语言分别设置。"
        >
          {[0, 1, 2].map((i) => (
            <StudioSelect
              key={i}
              ariaLabel={`说话者 ${i + 1} 音色`}
              disabled={disabled}
              value={b.voiceIds?.[i] || ''}
              onChange={(e) => {
                const next = [...(b.voiceIds || [])]
                next[i] = e.target.value
                change({ voiceIds: next.filter(Boolean) })
              }}
            >
              <option value="">不指定</option>
              {[
                'eve',
                'leo',
                'ara',
                'rex',
                'sal',
                'carina',
                'zagan',
                'helix',
                'orion',
                'luna',
                'iris',
                'altair',
                'zenith',
                'perseus',
                'helios',
                'lux',
                'kepler',
                'rigel',
                'cosmo',
                'celeste',
                'ursa',
                'sirius',
                'lumen',
                'castor',
                'naksh',
                'atlas',
                'aurora',
                'liora',
              ].map((v) => (
                <option key={v} value={v}>
                  {v}
                </option>
              ))}
            </StudioSelect>
          ))}
        </Field>
      )}
    </>
  )
}
