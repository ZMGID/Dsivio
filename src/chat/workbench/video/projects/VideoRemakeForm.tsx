import { VideoBriefLayout, RemakeProductFields, VideoSourceFields, type VideoFormProps } from './VideoFormFields'
export function VideoRemakeForm(props: VideoFormProps) { return <VideoBriefLayout {...props} isAnalysis remake extraMedia={<RemakeProductFields {...props} />} media={<VideoSourceFields {...props} />} /> }
