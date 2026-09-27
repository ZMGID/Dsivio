import { useState } from 'react'
import { Button } from '../../../components/Button'
import { useLang, useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchPage } from '../WorkbenchPage'
import './roleTemplates.css'

const portraits = [
  ['01', '东亚女性 · 28 岁', 'East Asian woman · 28'],
  ['02', '西非男性 · 36 岁', 'West African man · 36'],
  ['03', '南亚女性 · 47 岁', 'South Asian woman · 47'],
  ['04', '拉美男性 · 24 岁', 'Latin American man · 24'],
  ['05', '中东女性 · 33 岁', 'Middle Eastern woman · 33'],
  ['06', '东亚男性 · 68 岁', 'East Asian man · 68'],
  ['07', '非洲裔女性 · 22 岁', 'Black woman · 22'],
  ['08', '欧洲女性 · 41 岁', 'European woman · 41'],
  ['09', '东南亚男性 · 29 岁', 'Southeast Asian man · 29'],
  ['10', '拉美原住民女性 · 53 岁', 'Indigenous Latin American woman · 53'],
  ['11', '欧洲男性 · 61 岁', 'European man · 61'],
  ['12', '非洲裔中性形象 · 27 岁', 'Black nonbinary adult · 27'],
  ['13', '东亚男性 · 34 岁', 'East Asian man · 34'],
  ['14', '中东男性 · 45 岁', 'Middle Eastern man · 45'],
  ['15', '南亚女性 · 72 岁', 'South Asian woman · 72'],
] as const

export function RolesPage() {
  const t = useT()
  const en = useLang() === 'en'
  const [notice, setNotice] = useState('')

  return (
    <WorkbenchPage
      className="content-role-templates"
      crumb={t.workbenchGroupContent}
      title={t.workbenchNavRoles}
      subtitle={t.workbenchRolesSubtitle}
      actions={<Button size="sm" onClick={() => setNotice(t.workbenchRolesSoon)}>{t.workbenchRolesAdd}</Button>}
    >
      {notice ? <p className="workbench-inline-note">{notice}</p> : null}
      <WorkbenchCard>
        <div className="role-template-grid">
          {portraits.map(([id, zh, english], index) => {
            const label = en ? english : zh
            return <figure key={id} className="role-template-card">
              <img src={`/role-templates/avatar-${id}.webp`} alt={label} loading={index < 6 ? 'eager' : 'lazy'} decoding="async" />
              <figcaption>{label}</figcaption>
            </figure>
          })}
        </div>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
