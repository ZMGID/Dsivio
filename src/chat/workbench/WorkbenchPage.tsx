import type { ReactNode } from 'react'

/** 工作台中心页外框：面包屑 + 蓝标题 + 可选说明。页面自己管卡片。 */
export function WorkbenchPage({
  crumb,
  title,
  subtitle,
  actions,
  children,
}: {
  crumb?: string
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
            {crumb ? (
              <p className="workbench-crumb">
                <span>{crumb}</span>
                <span className="workbench-crumb-sep" aria-hidden="true">/</span>
                <span>{title}</span>
              </p>
            ) : null}
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
  fill,
  children,
}: {
  title?: string
  extra?: ReactNode
  fill?: boolean
  children: ReactNode
}) {
  return (
    <section className={`workbench-card-block${fill ? ' workbench-card-block--fill' : ''}`}>
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

export function WorkbenchEmpty({
  icon,
  title,
  children,
}: {
  icon?: ReactNode
  title?: string
  children?: ReactNode
}) {
  return (
    <div className="workbench-empty">
      {icon ? <div className="workbench-empty-icon">{icon}</div> : null}
      {title ? <p className="workbench-empty-title">{title}</p> : null}
      {children ? (
        <p className={title ? 'workbench-empty-hint' : 'workbench-empty-title'}>{children}</p>
      ) : null}
    </div>
  )
}
