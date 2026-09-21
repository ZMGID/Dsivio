import { useRef, useState } from 'react'
import type { Lang } from '../../components/i18n'
import { primaryAction, type MarketItem } from './types'
import type { MarketActions } from './MarketPage'
export const marketText = (lang: Lang, zh: string, en: string) => lang === 'en' ? en : zh
export function useMarketAction(actions: Pick<MarketActions, 'onInstall' | 'onUse'>) {
  const [busyIds, setBusyIds] = useState<Set<string>>(new Set())
  const locks = useRef(new Set<string>())
  const [error, setError] = useState('')
  const run = async (id: string, task: () => Promise<unknown>) => {
    if (locks.current.has(id)) return
    locks.current.add(id); setBusyIds(new Set(locks.current)); setError('')
    try { await task() } catch (e) { setError(String(e)) }
    finally { locks.current.delete(id); setBusyIds(new Set(locks.current)) }
  }
  const use = (item: MarketItem, newChat = true) => run(item.id, async () => {
    if (item.local?.status === 'ready' && primaryAction(item) !== 'repair') {
      await actions.onUse(item.local, newChat)
    } else await actions.onInstall(item.id)
  })
  return { busyIds, error, run, use }
}
export function actionLabel(item: MarketItem, lang: Lang) {
  const action = primaryAction(item)
  return ({ repair: marketText(lang, '重新配置', 'Reconfigure'), install: marketText(lang, '安装', 'Install'), continue: marketText(lang, '继续安装', 'Continue setup'), 'enable-use': marketText(lang, '加载并使用', 'Load and use'), use: marketText(lang, '使用', 'Use'), unavailable: marketText(lang, '暂不可用', 'Unavailable') })[action]
}
