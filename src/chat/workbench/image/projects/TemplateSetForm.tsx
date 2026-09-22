import { TemplateInputs, ProductMaterials, ImageRequirement, ImageOutputOptions, ImageExtraFields, ImageStart, type ImageFormProps } from './ImageFormFields'
export function TemplateSetForm(props: ImageFormProps) {
 return <div className="if-form if-form--split">
  <TemplateInputs {...props} />
  <ProductMaterials {...props} title="商品素材"  />
  <ImageRequirement {...props} placeholder="有哪些额外要求？不填则沿用模板。" />
  <ImageOutputOptions {...props}  />
  <ImageExtraFields {...props} />
  <ImageStart {...props} nextStep="按所选模板试做样品，确认后继续生成" />
 </div>
}
