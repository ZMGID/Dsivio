import VideoProjectWorkspace from './projects/VideoProjectWorkspace'
import { VideoEditForm } from './projects/VideoEditForm'

export function VideoEditPage() {
  return <VideoProjectWorkspace feature="editing" BriefForm={VideoEditForm} />
}
