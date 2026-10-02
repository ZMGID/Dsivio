import { TemplateInputs, ProductMaterials, ImageRequirement, ImageOutputOptions, ImageExtraFields, ImageStart, type ImageFormProps } from './ImageFormFields'
export function CloneImageForm(props: ImageFormProps) {
 return <div className="if-form if-form--split">
  <TemplateInputs {...props} />
  <ProductMaterials {...props} title="要换进去的商品"  />
  <ImageRequirement {...props} placeholder="有哪些额外要求？不填则保留原版式，只替换商品。" />
  <ImageOutputOptions {...props} followExample />
  <ImageExtraFields {...props} />
  <ImageStart {...props} nextStep="先查看并确认换货方案，再生成样品；满意后继续整批生成" />
 </div>
}
