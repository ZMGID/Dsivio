import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Lang } from '../../components/i18n'
import { open } from '@tauri-apps/plugin-dialog'
import {
  approveImPairing,
  beginImSetup,
  cancelImSetup,
  clearImCredentials,
  commitImSetup,
  denyImPairing,
  getImStatus,
  listImApprovedUsers,
  listImPairingRequests,
  pollImSetup,
  reconnectIm,
  revokeImUser,
  saveImCredentials,
  subscribeImPairing,
  subscribeImStatus,
} from '../../api/im'
import { IM_SETUP_POLL_MS, ImTab } from './ImTab'
import type { ImConfig, ImSetupSession, ImStatus } from '../../api/im'
import { makeProvider } from './testFixtures'

vi.mock('../../api/im', () => ({
  getImStatus: vi.fn(),
  saveImCredentials: vi.fn(),
  clearImCredentials: vi.fn(),
  reconnectIm: vi.fn(),
  listImPairingRequests: vi.fn(),
  listImApprovedUsers: vi.fn(),
  approveImPairing: vi.fn(),
  denyImPairing: vi.fn(),
  revokeImUser: vi.fn(),
  beginImSetup: vi.fn(),
  pollImSetup: vi.fn(),
  cancelImSetup: vi.fn(),
  commitImSetup: vi.fn(),
  subscribeImStatus: vi.fn(async () => () => {}),
  subscribeImPairing: vi.fn(async () => () => {}),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))

vi.mock('./imQr', () => ({
  authorizationQrDataUrl: vi.fn(async () => 'data:image/png;base64,local-qr'),
}))

function deferred<T>() {
  let resolve: (value: T) => void = () => {}
  let reject: (error: unknown) => void = () => {}
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

function backendConfig(): ImConfig {
  return {
    feishu: {
      enabled: false,
      appId: 'cli_existing',
      domain: 'feishu',
      connectionMode: 'websocket',
      webhook: { host: '127.0.0.1', port: 8765, path: '/feishu/webhook' },
      access: { dmPolicy: 'pairing', groupPolicy: 'allowlist', allowedUsers: ['ou_old'], allowedGroups: ['oc_old'], groupUsers: { oc_old: ['ou_old'] } },
      requireMention: true,
      homeChannel: 'oc_home',
    },
    wecom: {
      enabled: false,
      botId: 'bot_existing',
      websocketUrl: 'wss://openws.work.weixin.qq.com',
      access: { dmPolicy: 'pairing', groupPolicy: 'allowlist', allowedUsers: [], allowedGroups: [], groupUsers: {} },
      homeChannel: '',
    },
    wecomCallback: {
      enabled: true,
      corpId: 'ww_existing',
      agentId: '1001',
      webhook: { host: '127.0.0.1', port: 8645, path: '/wecom/callback' },
      access: { dmPolicy: 'disabled', groupPolicy: 'disabled', allowedUsers: [], allowedGroups: [], groupUsers: {} },
      homeChannel: 'callback-home',
    },
    agent: {
      assistantId: 'assistant-1',
      providerId: 'provider-1',
      model: 'model-1',
      workingDirectory: '/tmp/start',
      groupSessionsPerUser: true,
      streaming: true,
    },
  }
}

function status(partial: Partial<ImStatus> & Pick<ImStatus, 'platform' | 'state'>): ImStatus {
  return {
    message: '',
    credentialsConfigured: false,
    webhookUrl: '',
    lastMessageAt: null,
    ...partial,
  }
}

function setupSession(partial: Partial<ImSetupSession> = {}): ImSetupSession {
  return {
    id: 'setup-1',
    platform: 'feishu',
    url: 'https://accounts.feishu.cn/oauth/device?user_code=real-code',
    status: 'pending',
    expiresAt: Date.now() + 60_000,
    message: 'scan-now',
    identity: null,
    ...partial,
  }
}

function withStatus(status: string, partial: Partial<ImSetupSession> = {}): ImSetupSession {
  return { ...setupSession(partial), status: status as ImSetupSession['status'] }
}

function identity(partial: Partial<NonNullable<ImSetupSession['identity']>> = {}) {
  return {
    appId: 'cli_from_scan',
    botId: '',
    domain: 'lark' as const,
    ownerId: 'ou_owner',
    botName: 'Desk Bot',
    ...partial,
  }
}

function renderIm(initial: ImConfig | null = backendConfig(), lang: Lang = 'zh', onFlush: () => Promise<boolean> = async () => true) {
  let current = initial
  let mounted = true
  const providers = [makeProvider({ id: 'provider-2', name: 'Fixture', availableModels: ['model-2'], enabledModels: ['model-2'] })]
  const onChange = (next: ImConfig) => {
    current = next
    if (mounted) view.rerender(<ImTab lang={lang} config={current} providers={providers} onChange={onChange} onFlush={onFlush} />)
  }
  const view = render(<ImTab lang={lang} config={current} providers={providers} onChange={onChange} onFlush={onFlush} />)
  return {
    onChange,
    config: () => {
      if (current == null) throw new Error('Fixture has no IM configuration')
      return current
    },
    unmount: () => {
      mounted = false
      view.unmount()
    },
  }
}

function openAdvanced() {
  fireEvent.click(screen.getByText('高级设置'))
}

let statusListener: (() => void) | null = null
let pairingListener: (() => void) | null = null
let unlistenStatus: ReturnType<typeof vi.fn>
let unlistenPairing: ReturnType<typeof vi.fn>

function installListeners() {
  statusListener = null
  pairingListener = null
  unlistenStatus = vi.fn()
  unlistenPairing = vi.fn()
  vi.mocked(getImStatus).mockResolvedValue([])
  vi.mocked(cancelImSetup).mockResolvedValue(undefined)
  vi.mocked(commitImSetup).mockResolvedValue(setupSession({ status: 'completed', message: 'stored' }))
  vi.mocked(listImPairingRequests).mockResolvedValue([])
  vi.mocked(listImApprovedUsers).mockResolvedValue([])
  vi.mocked(subscribeImStatus).mockImplementation(async (listener) => {
    statusListener = listener
    return () => { unlistenStatus() }
  })
  vi.mocked(subscribeImPairing).mockImplementation(async (listener) => {
    pairingListener = listener
    return () => { unlistenPairing() }
  })
}

describe('ImTab', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    window.confirm = vi.fn(() => false)
    installListeners()
  })

  it('shows two connection cards and hides manual configuration until advanced settings are opened', async () => {
    const user = userEvent.setup()
    renderIm()
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    const wecom = within(screen.getByRole('region', { name: '企业微信机器人' }))
    expect(feishu.getByRole('button', { name: '扫码连接 飞书' })).toBeInTheDocument()
    expect(wecom.getByRole('button', { name: '扫码连接 企业微信' })).toBeInTheDocument()
    expect(screen.queryAllByRole('textbox')).toHaveLength(0)
    expect(screen.queryAllByRole('switch')).toHaveLength(0)
    expect(screen.queryByRole('region', { name: '企业微信自建应用' })).toBeNull()
    expect(screen.queryByRole('region', { name: '配对' })).toBeNull()
    expect(screen.queryByLabelText('飞书 App ID')).toBeNull()
    expect(screen.queryByLabelText('助手 ID')).toBeNull()

    await user.click(screen.getByText('高级设置'))
    expect(screen.getByLabelText('飞书 App ID')).toBeInTheDocument()
    expect(screen.getByLabelText('助手 ID')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '模型' })).toBeInTheDocument()
    expect(screen.getByRole('region', { name: '企业微信自建应用' })).toBeInTheDocument()
  })

  it('edits access, webhook, and the shared model from advanced settings without storing secrets', async () => {
    const user = userEvent.setup()
    const view = renderIm()
    openAdvanced()

    fireEvent.change(screen.getByLabelText('飞书 App ID'), { target: { value: 'cli_new' } })
    await user.click(screen.getByRole('button', { name: '飞书域名' }))
    await user.click(screen.getByRole('option', { name: 'Lark' }))
    await user.click(screen.getByRole('button', { name: '飞书连接方式' }))
    await user.click(screen.getByRole('option', { name: 'Webhook' }))
    fireEvent.change(screen.getByLabelText('飞书 / Lark Webhook 主机'), { target: { value: '0.0.0.0' } })
    fireEvent.change(screen.getByLabelText('飞书 / Lark Webhook 端口'), { target: { value: '8788' } })
    fireEvent.change(screen.getByLabelText('飞书 / Lark Webhook 路径'), { target: { value: '/hook' } })
    await user.click(screen.getByRole('button', { name: '飞书 / Lark 私聊策略' }))
    await user.click(screen.getByRole('option', { name: '开放' }))
    await user.click(screen.getByRole('button', { name: '飞书 / Lark 群策略' }))
    await user.click(screen.getByRole('option', { name: '关闭' }))
    fireEvent.change(screen.getByLabelText('飞书 / Lark 允许的用户'), { target: { value: 'ou_a, ou_b' } })
    fireEvent.change(screen.getByLabelText('飞书 / Lark 允许的群'), { target: { value: 'oc_a\noc_b' } })
    await user.click(screen.getByRole('button', { name: '飞书 / Lark 添加群用户规则' }))
    fireEvent.change(screen.getByLabelText('飞书 / Lark 群 ID 2'), { target: { value: 'oc_room' } })
    fireEvent.change(screen.getByLabelText('飞书 / Lark 该群允许的用户 2'), { target: { value: 'ou_room' } })
    await user.click(screen.getByRole('switch', { name: '群消息需要 @ 机器人' }))
    await user.click(screen.getByRole('switch', { name: '启用飞书' }))

    fireEvent.change(screen.getByLabelText('企业微信 Bot ID'), { target: { value: 'bot_new' } })
    fireEvent.change(screen.getByLabelText('企业微信 WebSocket 地址'), { target: { value: 'wss://example.test/wecom' } })
    fireEvent.change(screen.getByLabelText('企业 ID'), { target: { value: 'ww_new' } })
    fireEvent.change(screen.getByLabelText('应用 Agent ID'), { target: { value: '42' } })
    fireEvent.change(screen.getByLabelText('企业微信自建应用 Webhook 端口'), { target: { value: '8650' } })

    fireEvent.change(screen.getByLabelText('助手 ID'), { target: { value: 'assistant-2' } })
    await user.click(screen.getByRole('button', { name: '模型' }))
    await user.click(screen.getByRole('option', { name: '跟随桌面对话默认模型' }))
    fireEvent.change(screen.getByLabelText('工作目录'), { target: { value: '/work/im' } })
    await user.click(screen.getByRole('switch', { name: '群聊按用户分开会话' }))
    await user.click(screen.getByRole('switch', { name: '流式回复' }))
    fireEvent.change(screen.getByLabelText('飞书通知频道'), { target: { value: 'oc_notify' } })
    vi.mocked(open).mockResolvedValue('/picked/im')
    await user.click(screen.getByRole('button', { name: '选择工作目录' }))

    fireEvent.change(screen.getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    fireEvent.change(screen.getByLabelText('飞书 Encrypt Key'), { target: { value: 'encrypt-key' } })

    const config = view.config()
    expect(config.feishu).toMatchObject({
      enabled: true,
      appId: 'cli_new',
      domain: 'lark',
      connectionMode: 'webhook',
      requireMention: false,
      homeChannel: 'oc_notify',
      webhook: { host: '0.0.0.0', port: 8788, path: '/hook' },
      access: {
        dmPolicy: 'open',
        groupPolicy: 'disabled',
        allowedUsers: ['ou_a', 'ou_b'],
        allowedGroups: ['oc_a', 'oc_b'],
        groupUsers: { oc_old: ['ou_old'], oc_room: ['ou_room'] },
      },
    })
    expect(config.wecom).toMatchObject({ botId: 'bot_new', websocketUrl: 'wss://example.test/wecom' })
    expect(config.wecomCallback).toMatchObject({ corpId: 'ww_new', agentId: '42', webhook: { port: 8650 } })
    expect(config.agent).toMatchObject({
      assistantId: 'assistant-2',
      providerId: '',
      model: '',
      workingDirectory: '/picked/im',
      groupSessionsPerUser: false,
      streaming: false,
    })
    expect(JSON.stringify(config)).not.toContain('super-secret')
    expect(JSON.stringify(config)).not.toContain('encrypt-key')
    expect(screen.getByLabelText('飞书 App Secret')).toHaveValue('super-secret')
  })

  it('keeps a failed credential save visible and does not report success', async () => {
    const user = userEvent.setup()
    const view = renderIm()
    openAdvanced()
    vi.mocked(saveImCredentials).mockRejectedValue(new Error('keyring down'))
    fireEvent.change(screen.getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    expect(await screen.findByText('keyring down')).toBeInTheDocument()
    expect(screen.queryByText('凭证已保存')).toBeNull()
    expect(screen.getByLabelText('飞书 App Secret')).toHaveValue('super-secret')
    expect(JSON.stringify(view.config())).not.toContain('super-secret')
    expect(saveImCredentials).toHaveBeenCalledWith('feishu', 'cli_existing', expect.objectContaining({ secret: 'super-secret' }))
  })

  it('clears the password only after the keyring accepts it and shows backend status', async () => {
    const user = userEvent.setup()
    renderIm()
    openAdvanced()
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    vi.mocked(saveImCredentials).mockResolvedValue(undefined)
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'connected', message: 'socket-up', credentialsConfigured: true }),
    ])
    fireEvent.change(screen.getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    expect(await screen.findByText('凭证已配置')).toBeInTheDocument()
    expect(screen.getByText('凭证已保存')).toBeInTheDocument()
    expect(screen.getByLabelText('飞书 App Secret')).toHaveValue('')
    expect(feishu.getByText(/socket-up/)).toBeInTheDocument()
    expect(feishu.getByRole('status')).toHaveTextContent('已连接')
    expect(screen.queryByText('super-secret')).toBeNull()
  })

  it('writes settings before storing a secret and skips the keyring when that write fails', async () => {
    const user = userEvent.setup()
    const flushed = deferred<boolean>()
    const order: string[] = []
    const onFlush = vi.fn(async () => {
      order.push('flush')
      return flushed.promise
    })
    vi.mocked(saveImCredentials).mockImplementation(async () => { order.push('save') })
    renderIm(backendConfig(), 'zh', onFlush)
    openAdvanced()
    fireEvent.change(screen.getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    expect(saveImCredentials).not.toHaveBeenCalled()
    flushed.resolve(false)
    expect(await screen.findByText('设置没有保存，凭证没有写入。')).toBeInTheDocument()
    expect(saveImCredentials).not.toHaveBeenCalled()
    expect(screen.getByLabelText('飞书 App Secret')).toHaveValue('super-secret')
    expect(order).toEqual(['flush'])
  })

  it('stores the secret captured at click only when the saved id is unchanged', async () => {
    const user = userEvent.setup()
    const flushed = deferred<boolean>()
    const order: string[] = []
    vi.mocked(saveImCredentials).mockImplementation(async () => { order.push('save') })
    renderIm(backendConfig(), 'zh', async () => {
      order.push('flush')
      return flushed.promise
    })
    openAdvanced()
    fireEvent.change(screen.getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    expect(order).toEqual(['flush'])
    flushed.resolve(true)
    expect(await screen.findByText('凭证已保存')).toBeInTheDocument()
    expect(order).toEqual(['flush', 'save'])
    expect(saveImCredentials).toHaveBeenCalledWith('feishu', 'cli_existing', expect.objectContaining({ secret: 'super-secret' }))
  })

  it('does not store a secret when the bot id is empty or changes while settings are saving', async () => {
    const user = userEvent.setup()
    const empty = backendConfig()
    empty.feishu.appId = '   '
    const onFlush = vi.fn(async () => true)
    const view = renderIm(empty, 'zh', onFlush)
    openAdvanced()
    fireEvent.change(screen.getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    expect(await screen.findByText(/请先填写机器人 ID/)).toBeInTheDocument()
    expect(onFlush).toHaveBeenCalledOnce()
    expect(saveImCredentials).not.toHaveBeenCalled()

    const changed = deferred<boolean>()
    view.unmount()
    renderIm(backendConfig(), 'zh', () => changed.promise)
    openAdvanced()
    fireEvent.change(screen.getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    fireEvent.change(screen.getByLabelText('飞书 App ID'), { target: { value: 'cli_other' } })
    changed.resolve(true)
    expect(await screen.findByText('机器人 ID 已变化，凭证没有写入。')).toBeInTheDocument()
    expect(saveImCredentials).not.toHaveBeenCalled()
    expect(screen.getByLabelText('飞书 App Secret')).toHaveValue('super-secret')
  })

  it('saves two platforms at the same time and does not clear a newer secret when an older save finishes', async () => {
    const user = userEvent.setup()
    const first = deferred<void>()
    const second = deferred<void>()
    vi.mocked(saveImCredentials).mockImplementation((platform) => platform === 'feishu' ? first.promise : second.promise)
    renderIm()
    openAdvanced()
    fireEvent.change(screen.getByLabelText('飞书 App Secret'), { target: { value: 'secret-a' } })
    fireEvent.change(screen.getByLabelText('企业微信机器人 Secret'), { target: { value: 'wecom-secret' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    fireEvent.change(screen.getByLabelText('飞书 App Secret'), { target: { value: 'secret-b' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 企业微信' }))
    expect(saveImCredentials).toHaveBeenCalledWith('feishu', 'cli_existing', expect.objectContaining({ secret: 'secret-a' }))
    expect(saveImCredentials).toHaveBeenCalledWith('wecom', 'bot_existing', expect.objectContaining({ secret: 'wecom-secret' }))
    first.resolve()
    await waitFor(() => expect(screen.getByRole('button', { name: '保存凭证 飞书' })).toBeEnabled())
    expect(screen.getByLabelText('飞书 App Secret')).toHaveValue('secret-b')
    expect(screen.getByText('刚才提交的凭证已保存。输入框里后来改过的内容还没保存。')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '保存凭证 企业微信' })).toBeDisabled()
    second.resolve()
    await waitFor(() => expect(screen.getByRole('button', { name: '保存凭证 企业微信' })).toBeEnabled())
  })

  it('does not clear a secret typed for a new id while the previous save is finishing', async () => {
    const user = userEvent.setup()
    const landed = deferred<void>()
    vi.mocked(saveImCredentials).mockImplementation(() => landed.promise)
    renderIm()
    openAdvanced()
    fireEvent.change(screen.getByLabelText('飞书 App Secret'), { target: { value: 'secret-a' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    expect(saveImCredentials).toHaveBeenCalledWith('feishu', 'cli_existing', expect.objectContaining({ secret: 'secret-a' }))
    fireEvent.change(screen.getByLabelText('飞书 App ID'), { target: { value: 'cli_other' } })
    fireEvent.change(screen.getByLabelText('飞书 App Secret'), { target: { value: 'secret-b' } })
    landed.resolve()
    expect(await screen.findByText('机器人 ID 已变化，凭证没有写入。')).toBeInTheDocument()
    expect(screen.getByLabelText('飞书 App Secret')).toHaveValue('secret-b')
    expect(screen.queryByText('凭证已保存')).toBeNull()
  })

  it('clears only the identity captured before confirmation', async () => {
    const user = userEvent.setup()
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'error', message: 'missing secret', credentialsConfigured: true }),
    ])
    renderIm()
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    expect(await feishu.findByText('凭证已配置')).toBeInTheDocument()
    openAdvanced()
    await user.click(screen.getByRole('button', { name: '清除凭证 飞书' }))
    expect(clearImCredentials).not.toHaveBeenCalled()
    expect(feishu.getByText('凭证已配置')).toBeInTheDocument()

    window.confirm = vi.fn(() => {
      fireEvent.change(screen.getByLabelText('飞书 App ID'), { target: { value: 'cli_other' } })
      return true
    })
    await user.click(screen.getByRole('button', { name: '清除凭证 飞书' }))
    expect(await screen.findByText('机器人 ID 已变化，凭证没有写入。')).toBeInTheDocument()
    expect(clearImCredentials).not.toHaveBeenCalled()

    window.confirm = vi.fn(() => true)
    vi.mocked(clearImCredentials).mockRejectedValue(new Error('clear failed'))
    await user.click(screen.getByRole('button', { name: '清除凭证 飞书' }))
    expect(await screen.findByText('clear failed')).toBeInTheDocument()
    expect(clearImCredentials).toHaveBeenCalledWith('feishu', 'cli_other')
    expect(screen.queryByText('凭证已清除')).toBeNull()

    vi.mocked(clearImCredentials).mockResolvedValue(undefined)
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'error', message: 'missing secret', credentialsConfigured: false }),
    ])
    await user.click(screen.getByRole('button', { name: '清除凭证 飞书' }))
    expect(await screen.findByText('凭证未配置')).toBeInTheDocument()
    expect(screen.getByText('凭证已清除')).toBeInTheDocument()
  })

  it('shows backend connection states, reconnects, disables, and copies the callback url', async () => {
    const user = userEvent.setup()
    const writeText = vi.fn().mockResolvedValue(undefined)
    vi.spyOn(navigator.clipboard, 'writeText').mockImplementation(writeText)
    const config = backendConfig()
    config.feishu.enabled = true
    config.wecom.enabled = true
    const onFlush = vi.fn(async () => true)
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'connected', message: 'socket-up', credentialsConfigured: true, lastMessageAt: 1_700_000_000_000 }),
      status({ platform: 'wecom', state: 'retrying', message: 'backoff', credentialsConfigured: false }),
      status({ platform: 'wecom_callback', state: 'error', message: 'callback down', credentialsConfigured: true, webhookUrl: 'http://127.0.0.1:8645/wecom/callback' }),
    ])
    const view = renderIm(config, 'zh', onFlush)
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    const wecom = within(screen.getByRole('region', { name: '企业微信机器人' }))
    expect(await feishu.findByRole('status')).toHaveTextContent('已连接')
    expect(wecom.getByRole('alert')).toHaveTextContent('已启用但未配置凭证')
    expect(feishu.getByText(/2023/)).toBeInTheDocument()
    expect(wecom.getByRole('status')).toHaveTextContent('重连中')
    expect(screen.queryByRole('link', { name: 'http://127.0.0.1:8645/wecom/callback' })).toBeNull()
    openAdvanced()
    const callback = within(screen.getByRole('region', { name: '企业微信自建应用' }))
    expect(callback.getByRole('status')).toHaveTextContent('错误')
    expect(callback.getByRole('link', { name: 'http://127.0.0.1:8645/wecom/callback' })).toHaveAttribute('href', 'http://127.0.0.1:8645/wecom/callback')
    await user.click(callback.getByRole('button', { name: '复制回调地址' }))
    expect(writeText).toHaveBeenCalledWith('http://127.0.0.1:8645/wecom/callback')
    expect(await screen.findByText('回调地址已复制')).toBeInTheDocument()
    vi.mocked(reconnectIm).mockResolvedValue(undefined)
    await user.click(callback.getByRole('button', { name: '重新连接 企业微信自建应用' }))
    expect(reconnectIm).toHaveBeenCalledWith('wecom_callback')
    vi.mocked(reconnectIm).mockRejectedValue(new Error('reconnect refused'))
    await user.click(wecom.getByRole('button', { name: '重新连接 企业微信' }))
    expect(await screen.findByText('reconnect refused')).toBeInTheDocument()
    await user.click(feishu.getByRole('button', { name: '停用 飞书' }))
    await waitFor(() => expect(view.config().feishu.enabled).toBe(false))
    expect(onFlush).toHaveBeenCalled()
    expect(within(screen.getByRole('region', { name: '飞书 / Lark' })).queryByRole('button', { name: '停用 飞书' })).toBeNull()
  })

  it('restores the platform when disabling cannot be saved', async () => {
    const user = userEvent.setup()
    const config = backendConfig()
    config.feishu.enabled = true
    const view = renderIm(config, 'zh', async () => false)
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    await user.click(feishu.getByRole('button', { name: '停用 飞书' }))
    expect(await screen.findByText('设置没有保存，凭证没有写入。')).toBeInTheDocument()
    expect(view.config().feishu.enabled).toBe(true)
  })

  it('ignores a stale status response after a newer refresh', async () => {
    const first = deferred<ImStatus[]>()
    vi.mocked(getImStatus)
      .mockImplementationOnce(() => first.promise)
      .mockResolvedValueOnce([status({ platform: 'feishu', state: 'connected', message: 'socket-up', credentialsConfigured: true })])
    renderIm()
    await waitFor(() => expect(statusListener).toEqual(expect.any(Function)))
    statusListener?.()
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    expect(await feishu.findByText(/socket-up/)).toBeInTheDocument()
    first.resolve([status({ platform: 'feishu', state: 'error', message: 'stale-broker', credentialsConfigured: false })])
    await act(async () => { await Promise.resolve() })
    expect(screen.queryByText(/stale-broker/)).toBeNull()
    expect(feishu.getByText(/socket-up/)).toBeInTheDocument()
  })

  it('approves, denies, and revokes pairing only when a request is present', async () => {
    const user = userEvent.setup()
    const pending = { platform: 'feishu' as const, code: 'PAIR1', userId: 'ou_1', userName: 'Ada', createdAt: 1, expiresAt: Date.now() + 1000 }
    const approved = { platform: 'wecom' as const, identity: 'bot_existing', userId: 'zhang', userName: 'Zhang', approvedAt: 1_700_000_000_000 }
    vi.mocked(listImPairingRequests).mockResolvedValue([pending])
    vi.mocked(listImApprovedUsers).mockResolvedValue([approved])
    renderIm()
    expect(await screen.findByText(/Ada/)).toBeInTheDocument()
    expect(screen.getByText(/Zhang/)).toBeInTheDocument()
    expect(screen.getByText(/绑定 ID: bot_existing/)).toBeInTheDocument()

    vi.mocked(approveImPairing).mockRejectedValue(new Error('bad code'))
    await user.click(screen.getByRole('button', { name: '批准' }))
    expect(await screen.findByText('bad code')).toBeInTheDocument()
    expect(screen.getByText(/Ada/)).toBeInTheDocument()

    vi.mocked(approveImPairing).mockResolvedValue(undefined)
    vi.mocked(listImPairingRequests).mockResolvedValue([])
    await user.click(screen.getByRole('button', { name: '批准' }))
    await waitFor(() => expect(screen.queryByText(/Ada/)).toBeNull())
    expect(approveImPairing).toHaveBeenCalledWith('feishu', 'PAIR1')

    vi.mocked(listImPairingRequests).mockResolvedValue([pending])
    pairingListener?.()
    expect(await screen.findByText(/Ada/)).toBeInTheDocument()
    vi.mocked(denyImPairing).mockResolvedValue(undefined)
    vi.mocked(listImPairingRequests).mockResolvedValue([])
    await user.click(screen.getByRole('button', { name: '拒绝' }))
    await waitFor(() => expect(screen.queryByText(/Ada/)).toBeNull())
    expect(denyImPairing).toHaveBeenCalledWith('feishu', 'PAIR1')

    vi.mocked(revokeImUser).mockRejectedValue(new Error('revoke failed'))
    await user.click(screen.getByRole('button', { name: '撤销' }))
    expect(await screen.findByText('revoke failed')).toBeInTheDocument()
    expect(screen.getByText(/Zhang/)).toBeInTheDocument()

    vi.mocked(revokeImUser).mockResolvedValue(undefined)
    vi.mocked(listImApprovedUsers).mockResolvedValue([])
    await user.click(screen.getByRole('button', { name: '撤销' }))
    await waitFor(() => expect(screen.queryByText(/Zhang/)).toBeNull())
    expect(revokeImUser).toHaveBeenCalledWith('wecom', 'zhang')
    expect(screen.queryByRole('region', { name: '配对' })).toBeNull()
  })

  it('drops a stale pairing snapshot after a newer list arrives', async () => {
    const first = deferred<Array<{ platform: 'feishu', code: string, userId: string, userName: string, createdAt: number, expiresAt: number }>>()
    vi.mocked(listImPairingRequests)
      .mockImplementationOnce(() => first.promise)
      .mockResolvedValue([{ platform: 'feishu', code: 'NEW', userId: 'ou_new', userName: 'Neo', createdAt: 1, expiresAt: 1 }])
    renderIm()
    await waitFor(() => expect(pairingListener).toEqual(expect.any(Function)))
    pairingListener?.()
    expect(await screen.findByText(/Neo/)).toBeInTheDocument()
    first.resolve([{ platform: 'feishu', code: 'OLD', userId: 'ou_old', userName: 'Old', createdAt: 1, expiresAt: 1 }])
    await act(async () => { await Promise.resolve() })
    expect(screen.queryByText(/\bOLD\b/)).toBeNull()
    expect(screen.getByText(/Neo/)).toBeInTheDocument()
  })

  it('releases status and pairing subscriptions on unmount', async () => {
    const view = renderIm()
    await waitFor(() => expect(statusListener).toEqual(expect.any(Function)))
    const calls = vi.mocked(getImStatus).mock.calls.length
    view.unmount()
    await waitFor(() => expect(unlistenStatus).toHaveBeenCalled())
    expect(unlistenPairing).toHaveBeenCalled()
    statusListener?.()
    pairingListener?.()
    expect(vi.mocked(getImStatus).mock.calls.length).toBe(calls)
  })

  it('does not invent a config when the backend omits IM settings', async () => {
    renderIm(null)
    expect(screen.getByRole('alert')).toHaveTextContent('后端未返回 IM 配置')
    await act(async () => { await Promise.resolve() })
    expect(getImStatus).not.toHaveBeenCalled()
    expect(screen.queryByLabelText('飞书 App ID')).toBeNull()
    expect(screen.queryByRole('button', { name: '扫码连接 飞书' })).toBeNull()
  })
})

describe('ImTab setup session', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'] })
    vi.clearAllMocks()
    window.confirm = vi.fn(() => false)
    installListeners()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  async function settle() {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0)
    })
  }

  it('replaces a valid authorization QR with an error when the next link is insecure', async () => {
    vi.mocked(beginImSetup).mockResolvedValueOnce(setupSession())
    renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    expect(screen.getByRole('img', { name: '授权二维码' })).toBeInTheDocument()
    expect(screen.getByText(/打开飞书或 Lark/)).toBeInTheDocument()

    vi.mocked(beginImSetup).mockResolvedValueOnce(setupSession({ id: 'setup-http', url: 'http://accounts.feishu.cn/oauth' }))
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 企业微信' }))
    await settle()
    expect(screen.getByText('授权链接不是 https，已停止展示。')).toBeInTheDocument()
    expect(screen.queryByRole('img', { name: '授权二维码' })).toBeNull()
  })

  it('commits an authorized scan, preserves webhook settings, and shows only the backend connection', async () => {
    const snapshots: Array<{ appId: string, enabled: boolean, connectionMode: string, path: string }> = []
    const initial = backendConfig()
    initial.feishu.connectionMode = 'webhook'
    let statusCalls = 0
    vi.mocked(getImStatus).mockImplementation(async () => {
      statusCalls += 1
      return statusCalls === 1
        ? []
        : [status({ platform: 'feishu', state: 'connecting', message: 'opening', credentialsConfigured: true })]
    })
    const view = renderIm(initial, 'zh', async () => {
      const feishu = view.config().feishu
      snapshots.push({ appId: feishu.appId, enabled: feishu.enabled, connectionMode: feishu.connectionMode, path: feishu.webhook.path })
      return true
    })
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockResolvedValue(withStatus('authorized', {
      message: 'approved',
      identity: identity(),
    }))
    vi.mocked(commitImSetup).mockImplementation(async () => setupSession({
      status: 'completed',
      message: 'stored',
      identity: identity(),
    }))
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(snapshots.map((item) => item.enabled)).toEqual([false, true])
    expect(snapshots[0]).toMatchObject({ appId: 'cli_from_scan', connectionMode: 'websocket', path: '/feishu/webhook' })
    expect(commitImSetup).toHaveBeenCalledWith('setup-1', 'cli_from_scan')
    expect(view.config().feishu.enabled).toBe(true)
    expect(view.config().feishu.domain).toBe('lark')
    expect(view.config().wecom.botId).toBe('bot_existing')
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    expect(feishu.getByRole('status')).toHaveTextContent('连接中')
    expect(feishu.queryByRole('img', { name: '授权二维码' })).toBeNull()
    expect(screen.queryByText('已连接')).toBeNull()
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'connected', message: 'socket-up', credentialsConfigured: true }),
    ])
    statusListener?.()
    await settle()
    expect(feishu.getByRole('status')).toHaveTextContent('已连接')
  })

  it('keeps the scan recoverable when commit fails and restores the previous bot', async () => {
    const initial = backendConfig()
    initial.feishu.enabled = true
    initial.feishu.connectionMode = 'webhook'
    const view = renderIm(initial)
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockResolvedValue(withStatus('authorized', { identity: identity() }))
    vi.mocked(commitImSetup).mockRejectedValueOnce(new Error('keyring down'))
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(screen.getByText('keyring down')).toBeInTheDocument()
    expect(view.config().feishu).toMatchObject({
      appId: 'cli_existing',
      enabled: true,
      connectionMode: 'webhook',
      domain: 'feishu',
    })
    expect(view.config().feishu.webhook.path).toBe('/feishu/webhook')
    expect(screen.queryByText('已连接')).toBeNull()

    vi.mocked(commitImSetup).mockResolvedValue(setupSession({ status: 'completed', identity: identity() }))
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'connected', message: 'socket-up', credentialsConfigured: true }),
    ])
    fireEvent.click(screen.getByRole('button', { name: '重试保存授权' }))
    await settle()
    expect(view.config().feishu.enabled).toBe(true)
    expect(view.config().feishu.appId).toBe('cli_from_scan')
    expect(commitImSetup).toHaveBeenCalledTimes(2)
    expect(within(screen.getByRole('region', { name: '飞书 / Lark' })).getByRole('status')).toHaveTextContent('已连接')
  })

  it('restores the previous bot when authorization cannot be saved and keeps later edits on other fields', async () => {
    const initial = backendConfig()
    initial.feishu.enabled = true
    initial.feishu.connectionMode = 'webhook'
    const gate = deferred<boolean>()
    const seen: ImConfig[] = []
    let flushes = 0
    const view = renderIm(initial, 'zh', async () => {
      flushes += 1
      if (flushes === 1) await gate.promise
      seen.push(view.config())
      return flushes !== 1
    })
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockResolvedValue(withStatus('authorized', { identity: identity() }))
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    openAdvanced()
    fireEvent.change(screen.getByLabelText('企业微信 Bot ID'), { target: { value: 'bot_later' } })
    fireEvent.change(screen.getByLabelText('工作目录'), { target: { value: '/later' } })
    fireEvent.change(screen.getByLabelText('飞书通知频道'), { target: { value: 'oc_later' } })
    gate.resolve(false)
    await settle()
    expect(commitImSetup).not.toHaveBeenCalled()
    expect(view.config().feishu).toMatchObject({
      appId: 'cli_existing',
      enabled: true,
      connectionMode: 'webhook',
      domain: 'feishu',
      homeChannel: 'oc_later',
    })
    expect(view.config().wecom.botId).toBe('bot_later')
    expect(view.config().agent.workingDirectory).toBe('/later')
    expect(seen.at(-1)?.feishu.appId).toBe('cli_existing')
    expect(seen.at(-1)?.wecom.botId).toBe('bot_later')
    expect(screen.getByText('设置没有保存，凭证没有写入。')).toBeInTheDocument()
    expect(screen.queryByText('已连接')).toBeNull()
  })

  it('cancels an in-flight authorization without leaving the unscanned bot in place', async () => {
    const initial = backendConfig()
    initial.feishu.enabled = true
    const commit = deferred<ImSetupSession>()
    const view = renderIm(initial)
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockResolvedValue(withStatus('authorized', { identity: identity() }))
    vi.mocked(commitImSetup).mockImplementationOnce(() => commit.promise)
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    openAdvanced()
    fireEvent.change(screen.getByLabelText('企业微信 Bot ID'), { target: { value: 'bot_later' } })
    fireEvent.click(screen.getByRole('button', { name: '取消扫码' }))
    commit.resolve(setupSession({ status: 'cancelled', identity: identity() }))
    await settle()
    expect(view.config().feishu).toMatchObject({ appId: 'cli_existing', enabled: true, connectionMode: 'websocket' })
    expect(view.config().wecom.botId).toBe('bot_later')
    expect(screen.queryByText('已连接')).toBeNull()
  })

  it('keeps a saved authorization retryable when enabling it cannot be saved', async () => {
    let flushes = 0
    const view = renderIm(backendConfig(), 'zh', async () => {
      flushes += 1
      return flushes !== 2
    })
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockResolvedValue(withStatus('authorized', { identity: identity() }))
    vi.mocked(commitImSetup).mockResolvedValue(setupSession({ status: 'completed', identity: identity() }))
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(screen.getByText('授权已保存，但启用没有写入。可以重试，当前还不是已连接。')).toBeInTheDocument()
    expect(view.config().feishu).toMatchObject({ appId: 'cli_from_scan', enabled: false, connectionMode: 'websocket' })
    expect(screen.queryByText('已连接')).toBeNull()
    expect(commitImSetup).toHaveBeenCalledTimes(1)

    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'connected', message: 'socket-up', credentialsConfigured: true }),
    ])
    fireEvent.click(screen.getByRole('button', { name: '重试保存授权' }))
    await settle()
    expect(commitImSetup).toHaveBeenCalledTimes(1)
    expect(view.config().feishu.enabled).toBe(true)
    expect(within(screen.getByRole('region', { name: '飞书 / Lark' })).getByRole('status')).toHaveTextContent('已连接')
  })

  it('does not commit when the id changes after the authorized draft is saved', async () => {
    const flushed = deferred<boolean>()
    const view = renderIm(backendConfig(), 'zh', () => flushed.promise)
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockResolvedValue(withStatus('authorized', { identity: identity() }))
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    openAdvanced()
    fireEvent.change(screen.getByLabelText('飞书 App ID'), { target: { value: 'cli_typed' } })
    flushed.resolve(true)
    await settle()
    expect(commitImSetup).not.toHaveBeenCalled()
    expect(screen.getByText('机器人 ID 已变化，凭证没有写入。')).toBeInTheDocument()
    expect(view.config().feishu.appId).toBe('cli_typed')
    expect(view.config().feishu.enabled).toBe(false)
  })

  it('cancels the previous platform when another scan starts and ignores its late result', async () => {
    const late = deferred<ImSetupSession>()
    vi.mocked(beginImSetup)
      .mockResolvedValueOnce(setupSession())
      .mockResolvedValueOnce(setupSession({ id: 'setup-wecom', platform: 'wecom', url: 'https://work.weixin.qq.com/auth?code=1' }))
    vi.mocked(pollImSetup).mockImplementation((id) => id === 'setup-1' ? late.promise : Promise.resolve(setupSession({
      id: 'setup-wecom',
      platform: 'wecom',
      status: 'denied',
      message: 'no',
    })))
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 企业微信' }))
    await settle()
    expect(cancelImSetup).toHaveBeenCalledWith('setup-1')
    late.resolve(withStatus('authorized', { identity: identity() }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(commitImSetup).not.toHaveBeenCalled()
    expect(view.config().feishu.appId).toBe('cli_existing')
    expect(within(screen.getByRole('region', { name: '企业微信机器人' })).getByText(/扫码被拒绝/)).toBeInTheDocument()
  })

  it('ignores a completed poll that arrives after cancel', async () => {
    const polled = deferred<ImSetupSession>()
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockImplementation(() => polled.promise)
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    fireEvent.click(screen.getByRole('button', { name: '取消扫码' }))
    await settle()
    polled.resolve(withStatus('authorized', { identity: identity({ appId: 'cli_late' }) }))
    await settle()
    expect(view.config().feishu.appId).toBe('cli_existing')
    expect(cancelImSetup).toHaveBeenCalledWith('setup-1')
    expect(commitImSetup).not.toHaveBeenCalled()
    expect(screen.queryByText(/cli_late/)).toBeNull()
  })

  it('cancels a setup that is still starting when the page unmounts', async () => {
    const started = deferred<ImSetupSession>()
    vi.mocked(beginImSetup).mockImplementation(() => started.promise)
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    view.unmount()
    started.resolve(setupSession())
    await settle()
    expect(cancelImSetup).toHaveBeenCalledWith('setup-1')
    expect(commitImSetup).not.toHaveBeenCalled()
  })

  it('does not commit an in-flight authorization that fails after unmount, and still enables one that completed', async () => {
    const failed = deferred<ImSetupSession>()
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockResolvedValue(withStatus('authorized', { identity: identity() }))
    vi.mocked(commitImSetup).mockImplementationOnce(() => failed.promise)
    const first = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    first.unmount()
    failed.resolve(setupSession({ status: 'cancelled', identity: identity() }))
    await settle()
    expect(first.config().feishu.appId).toBe('cli_existing')
    expect(first.config().feishu.enabled).toBe(false)

    const committed = deferred<ImSetupSession>()
    vi.mocked(beginImSetup).mockResolvedValue(setupSession({ id: 'setup-2' }))
    vi.mocked(pollImSetup).mockResolvedValue(withStatus('authorized', {
      id: 'setup-2',
      identity: identity(),
    }))
    vi.mocked(commitImSetup).mockImplementationOnce(() => committed.promise)
    const second = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    second.unmount()
    committed.resolve(setupSession({ id: 'setup-2', status: 'completed', identity: identity() }))
    await settle()
    expect(second.config().feishu.appId).toBe('cli_from_scan')
    expect(second.config().feishu.enabled).toBe(true)
    expect(second.config().feishu.connectionMode).toBe('websocket')
  })

  it('times out a pending scan from the expiry timer and does not apply an identity', async () => {
    vi.mocked(beginImSetup).mockResolvedValue(setupSession({ expiresAt: Date.now() + 500 }))
    vi.mocked(pollImSetup).mockResolvedValue(withStatus('authorized', { identity: identity() }))
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(500) })
    await settle()
    expect(screen.getByText(/扫码已超时/)).toBeInTheDocument()
    expect(cancelImSetup).toHaveBeenCalledWith('setup-1')
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(commitImSetup).not.toHaveBeenCalled()
    expect(view.config().feishu.appId).toBe('cli_existing')
  })

  it('clears the expiry timer and listeners when the page unmounts', async () => {
    vi.mocked(beginImSetup).mockResolvedValue(setupSession({ expiresAt: Date.now() + 500 }))
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    view.unmount()
    await settle()
    expect(unlistenStatus).toHaveBeenCalled()
    expect(unlistenPairing).toHaveBeenCalled()
    const calls = vi.mocked(cancelImSetup).mock.calls.length
    await act(async () => { await vi.advanceTimersByTimeAsync(1_000) })
    expect(vi.mocked(cancelImSetup).mock.calls.length).toBe(calls)
    expect(commitImSetup).not.toHaveBeenCalled()
  })

  it('shows denied, poll failure, and a raw completed poll without changing the draft', async () => {
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockResolvedValueOnce(setupSession({ status: 'denied', message: 'rejected' }))
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(screen.getByText(/扫码被拒绝/)).toBeInTheDocument()
    expect(commitImSetup).not.toHaveBeenCalled()
    expect(view.config().feishu.appId).toBe('cli_existing')

    vi.mocked(pollImSetup).mockRejectedValueOnce(new Error('poll broke'))
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(screen.getByText('poll broke')).toBeInTheDocument()
    expect(view.config().feishu.appId).toBe('cli_existing')

    vi.mocked(pollImSetup).mockResolvedValueOnce(setupSession({ status: 'completed', identity: identity() }))
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(screen.getByText('这次扫码没有提交。')).toBeInTheDocument()
    expect(commitImSetup).not.toHaveBeenCalled()
    expect(view.config().feishu.appId).toBe('cli_existing')
  })

  it('shows a begin failure and does not render a QR code', async () => {
    vi.mocked(beginImSetup).mockRejectedValue(new Error('setup unavailable'))
    renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    expect(screen.getByText('setup unavailable')).toBeInTheDocument()
    expect(screen.queryByRole('img', { name: '授权二维码' })).toBeNull()
  })
})
