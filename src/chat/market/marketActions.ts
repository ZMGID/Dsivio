import { createWindowStore, useWindowStore } from '../../utils/windowStore'
import type { Lang } from '../../components/i18n'
import { primaryAction, type MarketItem } from './types'
import type { MarketActions } from './MarketPage'
export const marketText = (lang: Lang, zh: string, en: string) => lang === 'en' ? en : zh
const actionState = createWindowStore({ busyIds: new Set<string>(), error: '' })

/** Installation survives navigation; remounts observe the same in-flight action. */
export function useMarketAction(actions: Pick<MarketActions, 'onInstall' | 'onUse'>) {
  const [{ busyIds, error }] = useWindowStore(actionState)
  const run = (id: string, task: () => Promise<unknown>) => actionState.run(id, async () => {
    actionState.setState(s => ({ ...s, busyIds: new Set([...s.busyIds, id]), error: '' }))
    try { await task() } catch (e) { actionState.setState(s => ({ ...s, error: String(e) })) }
    finally { actionState.setState(s => { const next = new Set(s.busyIds); next.delete(id); return { ...s, busyIds: next } }) }
  })
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
