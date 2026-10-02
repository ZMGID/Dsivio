import { MessageSquare } from 'lucide-react'
import { Button } from '../../../../components/Button'
import { requestNewChat } from '../../../requestNewChat'
import { dsimageChatPrompt } from './dsimageChatPrompt'
import type { ImageBrief } from './types'

export function UseChatForSetButton({ brief }: { brief: ImageBrief }) {
  return (
    <Button variant="ghost" onClick={() => requestNewChat(dsimageChatPrompt(brief))}>
      <MessageSquare size={15} />
      用对话做
    </Button>
  )
}
