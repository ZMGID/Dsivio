import { useState } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { ACCOUNT_KPIS } from './publishCatalog'

/**
 * 账号授权：先看已绑账号。开源版还没接第三方授权。
 */
export function AccountsPage() {
  const t = useT()
  const [notice, setNotice] = useState('')

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupPublish}
      title={t.workbenchNavVaccts}
      subtitle={t.workbenchVacctsSubtitle}
      actions={<Button size="sm" onClick={() => setNotice(t.workbenchVacctsSoon)}>{t.workbenchVacctsBind}</Button>}
    >
      <div className="workbench-kpi-grid">
        {ACCOUNT_KPIS.map((item) => (
          <div key={item.id} className="workbench-kpi">
            <span className="workbench-kpi-value">0</span>
            <span className="workbench-kpi-label">{t[item.label]}</span>
          </div>
        ))}
      </div>
      {notice ? <p className="workbench-inline-note">{notice}</p> : null}
      <WorkbenchCard title={t.workbenchVacctsList}>
        <div className="custom-scrollbar workbench-table-scroll">
          <table className="workbench-table">
            <thead>
              <tr>
                <th>{t.workbenchVacctsColPlatform}</th>
                <th>{t.workbenchVacctsColAccount}</th>
                <th>{t.workbenchShopsColStatus}</th>
                <th>{t.workbenchVacctsColFans}</th>
                <th>{t.workbenchVacctsColSync}</th>
                <th>{t.workbenchColAction}</th>
              </tr>
            </thead>
          </table>
        </div>
        <WorkbenchEmpty compact>{t.workbenchVacctsEmpty}</WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
