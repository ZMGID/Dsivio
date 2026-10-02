import ImageProjectWorkspace from './projects/ImageProjectWorkspace'
import { DetailImageForm } from './DetailImageForm'

export function DetailImagePage() {
  return <ImageProjectWorkspace feature="detail" Form={DetailImageForm} />
}