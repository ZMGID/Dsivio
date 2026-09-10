import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { resolveResource } from '@tauri-apps/api/path'
import { api, type SkillMeta } from '../api/tauri'
import { CompanySkillSquare } from './CompanySkillSquare'
import { companySkills, type CompanySkill } from './companySkills'

vi.mock('@tauri-apps/api/path', () => ({ resolveResource: vi.fn() }))
vi.mock('../api/tauri', () => ({ api: { chatSkillsImport: vi.fn() } }))
vi.mock('./ChatMarkdown', () => ({ ChatMarkdown: ({ content }: { content: string }) => <div>{content}</div> }))

const items: CompanySkill[] = [
  { id: 'report', name: '周报整理', description: '整理工作记录', category: '办公', author: '内部团队', version: '1.0.0', details: '先提供本周工作记录。', directory: 'report' },
  { id: 'listing', name: '商品文案', description: '编写商品详情', category: '电商', author: '内部团队', version: '1.0.0', details: '提供产品资料。', directory: 'listing' },
]
const installed: SkillMeta = { id: 'report', name: '周报整理', description: '', source: 'user', recommendedTools: [] }

describe('CompanySkillSquare', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(resolveResource).mockResolvedValue('/app/company-skills/report')
    vi.mocked(api.chatSkillsImport).mockResolvedValue({ success: true, skill: installed })
  })

  it('shows an honest empty state before skills are published', () => {
    render(<CompanySkillSquare skills={[]} disabledSkillIds={[]} loading={false} onLoaded={vi.fn()} items={[]} />)
    expect(screen.getByText('专业技能准备中')).toBeTruthy()
    expect(screen.queryByRole('button', { name: /^加载 / })).toBeNull()
  })

  it('filters by category and search, and reveals usage details', () => {
    render(<CompanySkillSquare skills={[]} disabledSkillIds={[]} loading={false} onLoaded={vi.fn()} items={items} />)
    fireEvent.click(screen.getByRole('button', { name: '办公' }))
    expect(screen.queryByText('商品文案')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: '查看详情' }))
    expect(screen.getByText('先提供本周工作记录。')).toBeTruthy()
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '不存在' } })
    expect(screen.queryByText('周报整理')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: '全部技能' }))
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '商品详情' } })
    expect(screen.getByText('商品文案')).toBeTruthy()
  })

  it('imports the bundled package once then enables it', async () => {
    const onLoaded = vi.fn().mockResolvedValue(undefined)
    render(<CompanySkillSquare skills={[]} disabledSkillIds={[]} loading={false} onLoaded={onLoaded} items={items} />)
    const button = screen.getByRole('button', { name: '加载 周报整理' })
    fireEvent.click(button)
    fireEvent.click(button)
    await waitFor(() => expect(onLoaded).toHaveBeenCalledWith('report'))
    expect(resolveResource).toHaveBeenCalledWith('company-skills/report')
    expect(api.chatSkillsImport).toHaveBeenCalledTimes(1)
    expect(api.chatSkillsImport).toHaveBeenCalledWith('/app/company-skills/report')
  })

  it('reflects existing installed state and enables disabled skills without reinstalling', async () => {
    const onLoaded = vi.fn().mockResolvedValue(undefined)
    const { rerender } = render(<CompanySkillSquare skills={[installed]} disabledSkillIds={[]} loading={false} onLoaded={onLoaded} items={items} />)
    expect((screen.getByRole('button', { name: '已加载 周报整理' }) as HTMLButtonElement).disabled).toBe(true)
    rerender(<CompanySkillSquare skills={[installed]} disabledSkillIds={['report']} loading={false} onLoaded={onLoaded} items={items} />)
    fireEvent.click(screen.getByRole('button', { name: '加载 周报整理' }))
    await waitFor(() => expect(onLoaded).toHaveBeenCalledWith('report'))
    expect(api.chatSkillsImport).not.toHaveBeenCalled()
  })

  it('shows failures and retries activation without importing twice', async () => {
    const onLoaded = vi.fn().mockRejectedValueOnce(new Error('保存失败')).mockResolvedValue(undefined)
    render(<CompanySkillSquare skills={[]} disabledSkillIds={[]} loading={false} onLoaded={onLoaded} items={items} />)
    fireEvent.click(screen.getByRole('button', { name: '加载 周报整理' }))
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('保存失败'))
    fireEvent.click(screen.getByRole('button', { name: '加载 周报整理' }))
    await waitFor(() => expect(onLoaded).toHaveBeenCalledTimes(2))
    expect(api.chatSkillsImport).toHaveBeenCalledTimes(1)
  })

  it('does not activate a failed or mismatched package', async () => {
    const onLoaded = vi.fn()
    vi.mocked(api.chatSkillsImport).mockResolvedValueOnce({ success: false, error: '文件不存在' })
    render(<CompanySkillSquare skills={[]} disabledSkillIds={[]} loading={false} onLoaded={onLoaded} items={items} />)
    fireEvent.click(screen.getByRole('button', { name: '加载 周报整理' }))
    await waitFor(() => expect(within(screen.getByRole('alert')).getByText(/文件不存在/)).toBeTruthy())
    vi.mocked(api.chatSkillsImport).mockResolvedValueOnce({ success: true, skill: { ...installed, id: 'wrong' } })
    fireEvent.click(screen.getByRole('button', { name: '加载 周报整理' }))
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('不匹配'))
    expect(onLoaded).not.toHaveBeenCalled()
  })

  it('requires unique IDs and valid package directories in the published catalog', () => {
    expect(new Set(companySkills.map((item) => item.id)).size).toBe(companySkills.length)
    for (const item of companySkills) {
      expect(item.directory).toMatch(/^[a-zA-Z0-9_-]+$/)
      expect(item.id).not.toBe('')
      expect(item.name).not.toBe('')
    }
  })
})
