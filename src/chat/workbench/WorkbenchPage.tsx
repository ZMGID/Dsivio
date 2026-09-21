import type { ReactNode } from 'react'

/** 工作台中心页的通用外框：标题行 + 可选说明 + 正文。页面自己管卡片和表格。 */
export function WorkbenchPage({
  title,
  subtitle,
  actions,
  children,
}: {
  title: string
  subtitle?: string
  actions?: ReactNode
  children: ReactNode
}) {
  return (
    <div className="custom-scrollbar workbench-page">
      <div className="workbench-page-inner">
        <header className="workbench-page-head">
          <div className="min-w-0">
            <h1 className="workbench-title">{title}</h1>
            {subtitle ? <p className="workbench-page-sub">{subtitle}</p> : null}
          </div>
          {actions ? <div className="workbench-page-actions">{actions}</div> : null}
        </header>
        {children}
      </div>
    </div>
  )
}

export function WorkbenchCard({
  title,
  extra,
  children,
}: {
  title?: string
  extra?: ReactNode
  children: ReactNode
}) {
  return (
    <section className="workbench-card-block">
      {(title || extra) && (
        <div className="workbench-card-block-head">
          {title ? <h2 className="workbench-card-block-title">{title}</h2> : <span />}
          {extra}
        </div>
      )}
      {children}
    </section>
  )
}

export function WorkbenchEmpty({ children }: { children: ReactNode }) {
  return <p className="workbench-empty">{children}</p>
}
