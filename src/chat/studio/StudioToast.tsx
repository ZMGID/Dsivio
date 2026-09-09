import type { ReactNode } from 'react'
import { X } from 'lucide-react'
import { IconButton } from '../../components/Button'

export function StudioToast({
  children,
  actions,
  tone = 'notice',
  onClose,
}: {
  children: ReactNode
  actions?: ReactNode
  tone?: 'notice' | 'error'
  onClose?: () => void
}) {
  return (
    <div
      role={tone === 'error' ? 'alert' : 'status'}
      className={`studio-toast${tone === 'error' ? ' is-error' : ''}`}
    >
      <div className="studio-toast-body">
        <span>{children}</span>
        {actions}
      </div>
      {onClose && (
        <IconButton label="关闭提示" onClick={onClose}>
          <X size={14} />
        </IconButton>
      )}
    </div>
  )
}
