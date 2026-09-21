import type { Lang } from '../../components/i18n'

export function CapabilityTextEditor({ value, onChange, lang }: { value: string; onChange: (value: string) => void; lang: Lang }) {
  const zh = lang === 'zh'
  return <div className="space-y-3">
    <p className="kv-row-desc">{zh ? '像写 TXT 一样填写即可，没有格式要求。内容自动保存，包含密钥的原文会随配置 JSON 导入、导出，并在安装应用包时提供给 AI。' : 'Plain text, no required format. Saved automatically and included verbatim, with keys, in configuration import/export and package setup.'}</p>
    <textarea
      aria-label={zh ? '能力配置文本' : 'Capability configuration text'}
      value={value}
      onChange={event => onChange(event.target.value)}
      spellCheck={false}
      className="min-h-[420px] w-full resize-y rounded-xl border border-neutral-200 bg-transparent p-4 font-mono text-sm leading-7 outline-none focus:border-blue-500 dark:border-neutral-700"
      placeholder={zh ? '例如：\n\n图片生成\n接口地址：\nAPI 密钥：\n模型：\n使用说明：\n\n视频生成\n接口地址：\nAPI 密钥：\n模型：\n使用说明：' : 'Image generation\nURL:\nAPI key:\nModel:\nInstructions:\n\nVideo generation\nURL:\nAPI key:\nModel:\nInstructions:'}
    />
  </div>
}
