import { Suspense, lazy, useEffect, useState, type LazyExoticComponent, type ComponentType } from 'react'
import { WorkbenchLanding } from './WorkbenchLanding'
import { workbenchFeature, type WorkbenchSubpageId } from './registry'
import { workbenchPageFromHash, type WorkbenchPageId } from './workbenchPages'

/** 每个功能页只创建一次 lazy 组件，切页回来不重新下载。 */
const lazyPages = new Map<WorkbenchSubpageId, LazyExoticComponent<ComponentType>>()

function lazyPage(page: WorkbenchSubpageId): LazyExoticComponent<ComponentType> {
  let component = lazyPages.get(page)
  if (!component) {
    const feature = workbenchFeature(page)
    component = lazy(() => feature.load().then((Page) => ({ default: Page })))
    lazyPages.set(page, component)
  }
  return component
}

/**
 * 工作台中心区。chatView 只有 workbench 一种，具体哪一页读 hash 后缀，
 * 页面组件来自注册表 `registry.ts`，这里不枚举功能。
 */
export function WorkbenchHome() {
  const [page, setPage] = useState<WorkbenchPageId>(workbenchPageFromHash)

  useEffect(() => {
    const sync = () => setPage(workbenchPageFromHash())
    window.addEventListener('hashchange', sync)
    return () => window.removeEventListener('hashchange', sync)
  }, [])

  if (page === 'home') {
    return <div className="flex min-h-0 min-w-0 flex-1 flex-col"><WorkbenchLanding /></div>
  }
  const Page = lazyPage(page)
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <Suspense fallback={null}>
        <Page key={page} />
      </Suspense>
    </div>
  )
}
