import { ProductMaterials, ImageRequirement, ImageOutputOptions, ImageExtraFields, ImageStart, type ImageFormProps } from './ImageFormFields'
export function BatchSetForm(props: ImageFormProps) {
 return <div className="if-form">

  <ProductMaterials {...props} title="商品素材" folders />
  <ImageRequirement {...props} placeholder="例如：这批商品用于店铺上新，每款做一套主图、卖点和场景图。" />
  <ImageOutputOptions {...props}  />
  <ImageExtraFields {...props} />
  <ImageStart {...props} nextStep="每类先试做最多 2 款，满意后再生成剩余商品" />
 </div>
}
