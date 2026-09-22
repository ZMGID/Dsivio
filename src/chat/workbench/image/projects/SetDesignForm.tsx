import { ProductMaterials, ImageRequirement, ImageOutputOptions, ImageExtraFields, ImageStart, type ImageFormProps } from './ImageFormFields'
export function SetDesignForm(props: ImageFormProps) {
 return <div className="if-form">

  <ProductMaterials {...props} title="商品素材"  />
  <ImageRequirement {...props} placeholder="例如：设计一套简洁的电商主图，突出材质和容量，使用中文。" />
  <ImageOutputOptions {...props}  />
  <ImageExtraFields {...props} />
  <ImageStart {...props} nextStep="先设计逐页方案，再生成样品" />
 </div>
}
