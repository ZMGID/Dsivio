import { WorkbenchMediaModelSelect } from './WorkbenchMediaModelSelect'
import type { MediaPoolKind } from '../../data/mediaModelPools'
import type { ReactNode } from 'react'

/** 工作台中心页外框：面包屑 + 蓝标题 + 可选说明。页面自己管卡片。 */
export function WorkbenchPage({
  crumb,
  crumbCurrent,
  title,
  className,
  subtitle,
  actions,
  mediaPool,
  fill,
  children,
}: {
  crumb?: string
  crumbCurrent?: string
  title: string
  className?: string
  subtitle?: string
  actions?: ReactNode
  mediaPool?: MediaPoolKind
  fill?: boolean
  children: ReactNode
}) {
  return (
    <div className={`custom-scrollbar workbench-page${fill ? ' workbench-page--fill' : ''}${className ? ` ${className}` : ''}`}>
      <div className="workbench-page-inner">
        <header className="workbench-page-head">
          <div className="min-w-0">
            {crumb ? (
              <p className="workbench-crumb">
                <span>{crumb}</span>
                <span className="workbench-crumb-sep" aria-hidden="true">/</span>
                <span>{crumbCurrent ?? title}</span>
              </p>
            ) : null}
            <h1 className="workbench-title">{title}</h1>
            {subtitle ? <p className="workbench-page-sub">{subtitle}</p> : null}
          </div>
          {actions ? <div className="workbench-page-actions">{actions}</div> : null}
        </header>
        {mediaPool ? <WorkbenchMediaModelSelect key={mediaPool} kind={mediaPool}>{children}</WorkbenchMediaModelSelect> : children}
      </div>
    </div>
  )
}

export function WorkbenchCard({
  title,
  hint,
  extra,
  fill,
  children,
}: {
  title?: string
  hint?: string
  extra?: ReactNode
  fill?: boolean
  children: ReactNode
}) {
  return (
    <section className={`workbench-card-block${fill ? ' workbench-card-block--fill' : ''}`}>
      {(title || extra || hint) && (
        <div className="workbench-card-block-head">
          {title || hint ? (
            <div className="min-w-0">
              {title ? <h2 className="workbench-card-block-title">{title}</h2> : null}
              {hint ? <p className="workbench-page-sub workbench-page-sub--flush">{hint}</p> : null}
            </div>
          ) : <span />}
          {extra}
        </div>
      )}
      {children}
    </section>
  )
}

export function WorkbenchCta({
  notice,
  children,
}: {
  notice?: string
  children: ReactNode
}) {
  return (
    <div className="workbench-cta-row">
      {notice ? <p className="workbench-inline-note">{notice}</p> : null}
      {children}
    </div>
  )
}

export function WorkbenchEmpty({
  icon,
  title,
  compact = false,
  children,
}: {
  icon?: ReactNode
  title?: string
  compact?: boolean
  children?: ReactNode
}) {
  return (
    <div className={`workbench-empty${compact ? ' workbench-empty--compact' : ''}`}>
      {icon ? <div className="workbench-empty-icon">{icon}</div> : null}
      {title ? <p className="workbench-empty-title">{title}</p> : null}
      {children ? (
        <p className={title ? 'workbench-empty-hint' : 'workbench-empty-title'}>{children}</p>
      ) : null}
    </div>
  )
}
