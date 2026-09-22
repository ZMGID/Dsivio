import { ProductMaterials, ImageRequirement, ImageOutputOptions, ImageExtraFields, ImageStart, type ImageFormProps } from './ImageFormFields'
export function FreeImageForm(props: ImageFormProps) {
 return <div className="if-form">

  <ProductMaterials {...props} title="参考图片" optional />
  <ImageRequirement {...props} placeholder="例如：把背景换成浅色木桌，保留商品原样，不要文字。" />
  <ImageOutputOptions {...props} auto />
  <ImageExtraFields {...props} />
  <ImageStart {...props} nextStep="生成后，可以直接说出你想修改的地方" />
 </div>
}
