import { open } from '@tauri-apps/plugin-dialog'
import { Button } from '../../components/Button'
import { Field, StudioSelect } from '../images/StudioPanels'
import type { VideoBrief } from './types'

export function VideoMediaOptions({
  brief: b,
  change,
  disabled,
  native,
  onError,
}: {
  brief: VideoBrief
  change: (v: Partial<VideoBrief>) => void
  disabled: boolean
  native: boolean
  onError: (v: string) => void
}) {
  const mode = b.inputMode || 'auto'
  const reference =
    mode === 'reference' ||
    (mode === 'auto' && (b.images.length > 1 || !!b.voiceIds?.length))
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
          ],
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
              resolution: '',
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
          单图驱动：1 张，可选 1080p。参考生成：最多 7 张，最高
          720p，不锁定首帧。
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
              {(b[field] || []).map((p, n) => (
                <div className="vs-actions" key={p}>
                  <span className="vs-path">
                    {n + 1}. {p.split(/[\\/]/).pop()}
                  </span>
                  <Button
                    size="sm"
                    disabled={disabled}
                    onClick={() =>
                      change({ [field]: b[field]?.filter((v) => v !== p) })
                    }
                  >
                    移除
                  </Button>
                </div>
              ))}
              <Button
                size="sm"
                disabled={disabled || !native || (b[field]?.length || 0) >= 3}
                onClick={() => void pick(field)}
              >
                添加{i ? '音频' : '视频'}
              </Button>
            </div>
          ))}
          <p className="vs-muted">
            视频、音频分别最多 3 段，每段 2–15 秒且各自总时长 ≤ 15
            秒。所有参考素材合计最多 12 个。
          </p>
        </>
      )}
      <Field
        label="声音与口播"
        hint={
          b.route === 'grok'
            ? '静音会关闭 API 音轨生成。'
            : '声音要求写入导演剧本与提示词，成片需检查模型执行效果。'
        }
      >
        <StudioSelect
          disabled={disabled}
          value={b.speechMode || 'dialogue'}
          onChange={(e) =>
            change({ speechMode: e.target.value as VideoBrief['speechMode'] })
          }
        >
          <option value="dialogue">有口播 / 对白</option>
          <option value="ambient">无口播，保留环境音与音乐</option>
          <option value="silent">静音</option>
        </StudioSelect>
      </Field>
      {(b.speechMode || 'dialogue') === 'dialogue' && (
        <Field
          label="口播文案（可选）"
          hint="留空由导演编写；填写后按原文保留。"
        >
          <textarea
            className="kv-textarea"
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
