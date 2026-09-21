import { useEffect, useState } from 'react'
import { useT } from '../../components/i18n'
import { ListingCheckPage } from './commerce/ListingCheckPage'
import { ListingPage } from './commerce/ListingPage'
import { ProductArchivePage } from './commerce/ProductArchivePage'
import { ShopBindingPage } from './commerce/ShopBindingPage'
import { ShopOverviewPage } from './commerce/ShopOverviewPage'
import { LookalikePage } from './sourcing/LookalikePage'
import { PickLibraryPage } from './sourcing/PickLibraryPage'
import { VideoRankPage } from './sourcing/VideoRankPage'
import { WorkflowPage } from './workflow/WorkflowPage'
import { workbenchPageFromHash, type WorkbenchPageId } from './workbenchPages'

function WorkbenchLanding() {
  const t = useT()
  return (
    <div className="custom-scrollbar workbench-home">
      <div className="workbench-home-inner">
        <h1 className="workbench-title">{t.productModeWorkbenchName}</h1>
      </div>
    </div>
  )
}

function renderPage(page: WorkbenchPageId) {
  switch (page) {
    case 'shops':
      return <ShopBindingPage />
    case 'overview':
      return <ShopOverviewPage />
    case 'products':
      return <ProductArchivePage />
    case 'listing':
      return <ListingPage />
    case 'check':
      return <ListingCheckPage />
    case 'workflows':
      return <WorkflowPage />
    case 'ranks':
      return <VideoRankPage />
    case 'match':
      return <LookalikePage />
    case 'picks':
      return <PickLibraryPage />
    default:
      return <WorkbenchLanding />
  }
}

/**
 * 工作台中心区。chatView 只有 workbench 一种，具体哪一页读 hash 后缀。
 */
export function WorkbenchHome() {
  const [page, setPage] = useState<WorkbenchPageId>(workbenchPageFromHash)

  useEffect(() => {
    const sync = () => setPage(workbenchPageFromHash())
    window.addEventListener('hashchange', sync)
    return () => window.removeEventListener('hashchange', sync)
  }, [])

  return <div className="flex min-h-0 min-w-0 flex-1 flex-col">{renderPage(page)}</div>
}
