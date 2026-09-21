import { useState } from 'react'
import { Users } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'

/**
 * 角色库：开源版没有预置角色，也还没接本地角色库。
 */
export function RolesPage() {
  const t = useT()
  const [notice, setNotice] = useState('')

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupContent}
      title={t.workbenchNavRoles}
      subtitle={t.workbenchRolesSubtitle}
      actions={<Button size="sm" onClick={() => setNotice(t.workbenchRolesSoon)}>{t.workbenchRolesAdd}</Button>}
    >
      {notice ? <p className="workbench-inline-note">{notice}</p> : null}
      <WorkbenchCard fill>
        <WorkbenchEmpty icon={<Users size={22} />} title={t.workbenchRolesEmpty}>
          {t.workbenchRolesEmptyHint}
        </WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
