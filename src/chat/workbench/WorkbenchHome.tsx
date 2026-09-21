import { useEffect, useState } from 'react'
import { useT } from '../../components/i18n'
import { ListingCheckPage } from './commerce/ListingCheckPage'
import { ListingPage } from './commerce/ListingPage'
import { ProductArchivePage } from './commerce/ProductArchivePage'
import { ShopBindingPage } from './commerce/ShopBindingPage'
import { ShopOverviewPage } from './commerce/ShopOverviewPage'
import { GraphicPostPage } from './copy/GraphicPostPage'
import { SeedArticlePage } from './copy/SeedArticlePage'
import { CloneImagePage } from './image/CloneImagePage'
import { DetailImagePage } from './image/DetailImagePage'
import { DressPage } from './image/DressPage'
import { EditImagePage } from './image/EditImagePage'
import { MainImagePage } from './image/MainImagePage'
import { MigratePage } from './image/MigratePage'
import { PosterPage } from './image/PosterPage'
import { RetouchPage } from './image/RetouchPage'
import { AvatarPage } from './video/AvatarPage'
import { DramaPage } from './video/DramaPage'
import { ShortsPage } from './video/ShortsPage'
import { SubtitlePage } from './video/SubtitlePage'
import { VideoClonePage } from './video/VideoClonePage'
import { VideoEditPage } from './video/VideoEditPage'
import { AccountsPage } from './publish/AccountsPage'
import { PublishDataPage } from './publish/PublishDataPage'
import { PublishLogsPage } from './publish/PublishLogsPage'
import { PublishPage } from './publish/PublishPage'
import { AssetLibraryPage } from './content/AssetLibraryPage'
import { RolesPage } from './content/RolesPage'
import { UsagePage } from './stats/UsagePage'
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
    case 'posts':
      return <GraphicPostPage />
    case 'articles':
      return <SeedArticlePage />
    case 'main':
      return <MainImagePage />
    case 'detail':
      return <DetailImagePage />
    case 'poster':
      return <PosterPage />
    case 'retouch':
      return <RetouchPage />
    case 'migrate':
      return <MigratePage />
    case 'dress':
      return <DressPage />
    case 'clone':
      return <CloneImagePage />
    case 'edit':
      return <EditImagePage />
    case 'shorts':
      return <ShortsPage />
    case 'avatar':
      return <AvatarPage />
    case 'drama':
      return <DramaPage />
    case 'vclone':
      return <VideoClonePage />
    case 'vedit':
      return <VideoEditPage />
    case 'subs':
      return <SubtitlePage />
    case 'publish':
      return <PublishPage />
    case 'vaccts':
      return <AccountsPage />
    case 'plogs':
      return <PublishLogsPage />
    case 'pdata':
      return <PublishDataPage />
    case 'roles':
      return <RolesPage />
    case 'assets':
      return <AssetLibraryPage />
    case 'usage':
      return <UsagePage />
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
