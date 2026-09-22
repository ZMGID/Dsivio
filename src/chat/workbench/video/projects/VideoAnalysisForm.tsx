import { VideoBriefLayout, VideoSourceFields, type VideoFormProps } from './VideoFormFields'
export function VideoAnalysisForm(props: VideoFormProps) { return <VideoBriefLayout {...props} isAnalysis media={<VideoSourceFields {...props} />} /> }
