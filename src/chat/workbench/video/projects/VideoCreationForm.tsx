import { VideoBriefLayout, VideoProductFields, type VideoFormProps } from './VideoFormFields'
export function VideoCreationForm(props: VideoFormProps) { return <VideoBriefLayout {...props}  media={<VideoProductFields {...props} />} /> }
