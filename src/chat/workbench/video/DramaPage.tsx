import VideoProjectWorkspace from './projects/VideoProjectWorkspace'
import { VideoDramaForm } from './projects/VideoDramaForm'

export function DramaPage() {
  return <VideoProjectWorkspace feature="drama" BriefForm={VideoDramaForm} />
}