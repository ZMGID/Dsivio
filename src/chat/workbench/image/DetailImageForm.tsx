import { ProductMaterials, ImageStart, type ImageFormProps } from './projects/ImageFormFields'
import { RequirementComposer } from './projects/RequirementComposer'
import { Field } from './projects/StudioPanels'
import { Select } from '../../../settings/public/controls'

const PLATFORMS = [
  { value: 'tb', label: '淘宝' },
  { value: 'jd', label: '京东' },
  { value: 'pdd', label: '拼多多' },
  { value: 'dy', label: '抖音' },
]
const RATIOS = [
  { value: '3:4', label: '3:4 竖版' },
  { value: '9:16', label: '9:16 竖版' },
]

/** 详情页：商品图、描述、平台和竖版比例，先规划模块再逐张生成。 */
export function DetailImageForm(props: ImageFormProps) {
  const { brief, onChange } = props
  return <div className="if-form">
    <ProductMaterials {...props} title="产品图" />
    <section className="if-requirements">
      <RequirementComposer label="产品描述" value={brief.requirement} onChange={(requirement) => onChange({ requirement })} placeholder="例如：儿童运动鞋，适合 3-8 岁，透气网面。" />
    </section>
    <div className="if-options">
      <Field label="平台">
        <Select value={brief.platform || 'tb'} ariaLabel="平台" onChange={(platform) => onChange({ platform })} options={PLATFORMS} />
      </Field>
      <Field label="比例">
        <Select value={brief.ratio} ariaLabel="比例" onChange={(ratio) => onChange({ ratio })} options={RATIOS} />
      </Field>
    </div>
    <ImageStart {...props} nextStep="先确认头图、卖点、参数、场景和尺寸模块，再逐张生成" />
  </div>
}
