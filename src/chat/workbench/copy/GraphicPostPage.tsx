import ImageProjectWorkspace from '../image/projects/ImageProjectWorkspace'
import { GraphicPostForm } from './GraphicPostForm'

export function GraphicPostPage() {
  return <ImageProjectWorkspace feature="post" Form={GraphicPostForm} />
}
