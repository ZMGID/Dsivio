import { Button } from '../../../../components/Button'
import { ProductMaterials, ImageRequirement, ImageOutputOptions, ImageExtraFields, ImageStart, type ImageFormProps } from './ImageFormFields'
export function FreeImageForm(props: ImageFormProps) {
 return <div className="if-form">

  {!props.brief.requirement && <div className="flex flex-wrap gap-2" aria-label="起步想法">
    {['制作简洁的商品白底主图，保留商品结构与颜色。', '把商品放在自然光生活场景中，突出使用细节。', '制作适合电商详情页的卖点图，留出标题区域。'].map(prompt =>
      <Button key={prompt} size="sm" onClick={() => props.onChange({ requirement: prompt })}>{prompt}</Button>)}
  </div>}
  <ProductMaterials {...props} title="参考图片" optional />
  <ImageRequirement {...props} placeholder="例如：把背景换成浅色木桌，保留商品原样，不要文字。" />
  <ImageOutputOptions {...props} auto />
  <ImageExtraFields {...props} />
  <ImageStart {...props} nextStep="生成后，可以直接说出你想修改的地方" />
 </div>
}
