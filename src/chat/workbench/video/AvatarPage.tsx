import VideoProjectWorkspace from './projects/VideoProjectWorkspace'
import { VideoAvatarForm } from './projects/VideoAvatarForm'

export function AvatarPage() {
  return <VideoProjectWorkspace feature="avatar" BriefForm={VideoAvatarForm} />
}
