import { ProductMaterials, ImageStart, type ImageFormProps } from '../image/projects/ImageFormFields'
import { RequirementComposer } from '../image/projects/RequirementComposer'
import { Field } from '../image/projects/StudioPanels'
import { Select } from '../../../settings/public/controls'
import { POST_RATIOS, POST_TEMPLATES } from './copyCatalog'

/** 图文带货：商品图、卖点、平台模板和比例，走图片项目的方案确认。 */
export function GraphicPostForm(props: ImageFormProps) {
  const { brief, onChange } = props
  return <div className="if-form">
    <ProductMaterials {...props} title="产品图" />
    <section className="if-requirements">
      <RequirementComposer label="带货需求描述" value={brief.requirement} onChange={(requirement) => onChange({ requirement })} placeholder="例如：主打夏季通勤、轻便透气，面向 25-35 岁通勤女性。" />
    </section>
    <div className="if-options">
      <Field label="发布平台模板">
        <Select value={brief.platform || 'xhs'} ariaLabel="发布平台模板" onChange={(platform) => onChange({ platform })} options={POST_TEMPLATES.map((item) => ({ value: item.id, label: { xhs: '小红书种草', douyin: '抖音图文', kuaishou: '快手好物', taobao: '淘宝逛逛', channels: '视频号图文', moments: '朋友圈带货' }[item.id] }))} />
      </Field>
      <Field label="比例">
        <Select value={brief.ratio} ariaLabel="比例" onChange={(ratio) => onChange({ ratio })} options={POST_RATIOS.map((item) => ({ value: item.id, label: item.id }))} />
      </Field>
    </div>
    <ImageStart {...props} nextStep="先生成可编辑的标题、正文、标签和每张配图方案" />
  </div>
}
