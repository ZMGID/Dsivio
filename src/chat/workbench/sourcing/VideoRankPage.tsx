import { useState } from 'react'
import { BarChart3, Flame, Heart, PlayCircle, Search, UserPlus } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Select } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'

type RankKind = 'all' | 'lowFan' | 'finish' | 'like' | 'follow'
type RankRange = '1h' | '1d' | '3d' | '7d'

const KIND_ICON = {
  all: BarChart3,
  lowFan: Flame,
  finish: PlayCircle,
  like: Heart,
  follow: UserPlus,
} as const

/**
 * 视频榜单：平台、榜种、时间、关键词。开源版还没接抖音榜，表头按活页抄，行保持空。
 */
export function VideoRankPage() {
  const t = useT()
  const [kind, setKind] = useState<RankKind>('all')
  const [range, setRange] = useState<RankRange>('1d')
  const [query, setQuery] = useState('')

  const kinds: { id: RankKind; label: string }[] = [
    { id: 'all', label: t.workbenchRankAll },
    { id: 'lowFan', label: t.workbenchRankLowFan },
    { id: 'finish', label: t.workbenchRankFinish },
    { id: 'like', label: t.workbenchRankLike },
    { id: 'follow', label: t.workbenchRankFollow },
  ]

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupSourcing}
      title={t.workbenchNavRanks}
      actions={(
        <div className="workbench-page-actions">
          <button type="button" className="workbench-chip is-active">{t.workbenchPlatformDouyinShort}</button>
          <button type="button" className="workbench-chip" disabled>{t.workbenchPlatformTiktokSoon}</button>
        </div>
      )}
    >
      <div className="workbench-tabs">
        {kinds.map((item) => {
          const Icon = KIND_ICON[item.id]
          return (
            <button
              key={item.id}
              type="button"
              className={`workbench-tab${kind === item.id ? ' is-active' : ''}`}
              onClick={() => setKind(item.id)}
            >
              <Icon size={15} />
              {item.label}
            </button>
          )
        })}
      </div>

      <WorkbenchCard fill>
        <div className="workbench-toolbar">
          <Select
            value={range}
            onChange={(value) => setRange(value as RankRange)}
            ariaLabel={t.workbenchRankRange}
            options={[
              { value: '1h', label: t.workbenchRank1h },
              { value: '1d', label: t.workbenchRank1d },
              { value: '3d', label: t.workbenchRank3d },
              { value: '7d', label: t.workbenchRank7d },
            ]}
          />
          <input
            className="workbench-search"
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t.workbenchRankSearch}
          />
          <Button size="sm" variant="primary">
            <Search size={14} />
            {t.workbenchRankSearchAction}
          </Button>
        </div>
        <WorkbenchEmpty icon={<BarChart3 size={22} />} title={t.workbenchRankEmpty}>
          {t.workbenchRankSoon}
        </WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
