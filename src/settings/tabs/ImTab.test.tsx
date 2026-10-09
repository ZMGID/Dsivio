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
  subscribeImStatus: vi.fn(async () => () => {}),
  subscribeImPairing: vi.fn(async () => () => {}),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))

vi.mock('./imQr', () => ({
  authorizationQrDataUrl: vi.fn(async (url: string) => `data:image/png;base64,${btoa(url)}`),
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

/** Canonical-shaped backend payload for the page. The page itself must not invent one. */
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

function renderIm(initial: ImConfig | null = backendConfig(), lang: Lang = 'zh', onFlush: () => Promise<boolean> = async () => true) {
  let current = initial
  const providers = [makeProvider({ id: 'provider-2', name: 'Fixture', availableModels: ['model-2'], enabledModels: ['model-2'] })]
  const onChange = (next: ImConfig) => {
    current = next
    view.rerender(<ImTab lang={lang} config={current} providers={providers} onChange={onChange} onFlush={onFlush} />)
  }
  const view = render(<ImTab lang={lang} config={current} providers={providers} onChange={onChange} onFlush={onFlush} />)
  return {
    onChange,
    config: () => {
      if (current == null) throw new Error('Fixture has no IM configuration')
      return current
    },
    unmount: () => view.unmount(),
  }
}

let statusListener: (() => void) | null = null
let pairingListener: (() => void) | null = null
let unlistenStatus: ReturnType<typeof vi.fn>
let unlistenPairing: ReturnType<typeof vi.fn>

describe('ImTab', () => {
  beforeEach(() => {
    statusListener = null
    pairingListener = null
    unlistenStatus = vi.fn()
    unlistenPairing = vi.fn()
    vi.clearAllMocks()
    window.confirm = vi.fn(() => false)
    vi.mocked(getImStatus).mockResolvedValue([])
    vi.mocked(cancelImSetup).mockResolvedValue(undefined)
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
  })

  it('edits platform identity, access, mention rules, and the shared assistant without storing secrets', async () => {
    const user = userEvent.setup()
    const view = renderIm()
    const feishu = () => within(screen.getByRole('region', { name: '飞书 / Lark' }))

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
    await user.click(feishu().getByRole('button', { name: '添加群用户规则' }))
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
    await user.click(screen.getByRole('option', { name: 'Fixture - model-2' }))
    fireEvent.change(screen.getByLabelText('工作目录'), { target: { value: '/work/im' } })
    await user.click(screen.getByRole('switch', { name: '群聊按用户分开会话' }))
    await user.click(screen.getByRole('switch', { name: '流式回复' }))
    fireEvent.change(screen.getByLabelText('飞书通知频道'), { target: { value: 'oc_notify' } })
    vi.mocked(open).mockResolvedValue('/picked/im')
    await user.click(screen.getByRole('button', { name: '选择工作目录' }))

    fireEvent.change(feishu().getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    fireEvent.change(feishu().getByLabelText('飞书 Encrypt Key'), { target: { value: 'encrypt-key' } })

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
      providerId: 'provider-2',
      model: 'model-2',
      workingDirectory: '/picked/im',
      groupSessionsPerUser: false,
      streaming: false,
    })
    expect(JSON.stringify(config)).not.toContain('super-secret')
    expect(JSON.stringify(config)).not.toContain('encrypt-key')
    expect(feishu().getByLabelText('飞书 App Secret')).toHaveValue('super-secret')
  })

  it('keeps a failed credential save visible and does not report success', async () => {
    const user = userEvent.setup()
    const view = renderIm()
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    vi.mocked(saveImCredentials).mockRejectedValue(new Error('keyring down'))
    fireEvent.change(feishu.getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    expect(await screen.findByText('keyring down')).toBeInTheDocument()
    expect(screen.queryByText('凭证已保存')).toBeNull()
    expect(feishu.getByLabelText('飞书 App Secret')).toHaveValue('super-secret')
    expect(JSON.stringify(view.config())).not.toContain('super-secret')
    expect(saveImCredentials).toHaveBeenCalledWith('feishu', expect.objectContaining({ secret: 'super-secret' }))
  })

  it('clears the password only after the keyring accepts it and shows the boolean status', async () => {
    const user = userEvent.setup()
    renderIm()
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    vi.mocked(saveImCredentials).mockResolvedValue(undefined)
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'connected', message: 'socket-up', credentialsConfigured: true }),
    ])
    fireEvent.change(feishu.getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    expect(await screen.findByText('凭证已配置')).toBeInTheDocument()
    expect(screen.getByText('凭证已保存')).toBeInTheDocument()
    expect(feishu.getByLabelText('飞书 App Secret')).toHaveValue('')
    expect(feishu.getByText(/socket-up/)).toBeInTheDocument()
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
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    fireEvent.change(feishu.getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    expect(saveImCredentials).not.toHaveBeenCalled()
    flushed.resolve(false)
    expect(await screen.findByText('设置没有保存，凭证没有写入。')).toBeInTheDocument()
    expect(saveImCredentials).not.toHaveBeenCalled()
    expect(feishu.getByLabelText('飞书 App Secret')).toHaveValue('super-secret')
    expect(order).toEqual(['flush'])
  })

  it('stores the secret only after the settings flush succeeds', async () => {
    const user = userEvent.setup()
    const flushed = deferred<boolean>()
    const order: string[] = []
    vi.mocked(saveImCredentials).mockImplementation(async () => { order.push('save') })
    renderIm(backendConfig(), 'zh', async () => {
      order.push('flush')
      return flushed.promise
    })
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    fireEvent.change(feishu.getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    expect(order).toEqual(['flush'])
    flushed.resolve(true)
    expect(await screen.findByText('凭证已保存')).toBeInTheDocument()
    expect(order).toEqual(['flush', 'save'])
    expect(saveImCredentials).toHaveBeenCalledWith('feishu', expect.objectContaining({ secret: 'super-secret' }))
  })

  it('does not store a secret when the bot id is still empty after settings are saved', async () => {
    const user = userEvent.setup()
    const config = backendConfig()
    config.feishu.appId = '   '
    const onFlush = vi.fn(async () => true)
    renderIm(config, 'zh', onFlush)
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    fireEvent.change(feishu.getByLabelText('飞书 App Secret'), { target: { value: 'super-secret' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    expect(await screen.findByText(/请先填写机器人 ID/)).toBeInTheDocument()
    expect(onFlush).toHaveBeenCalledOnce()
    expect(saveImCredentials).not.toHaveBeenCalled()
    expect(feishu.getByLabelText('飞书 App Secret')).toHaveValue('super-secret')
  })

  it('does not clear a newer secret when an older save finishes later', async () => {
    const user = userEvent.setup()
    const first = deferred<void>()
    vi.mocked(saveImCredentials).mockImplementationOnce(() => first.promise)
    renderIm()
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    const input = feishu.getByLabelText('飞书 App Secret')
    fireEvent.change(input, { target: { value: 'secret-a' } })
    await user.click(screen.getByRole('button', { name: '保存凭证 飞书' }))
    fireEvent.change(input, { target: { value: 'secret-b' } })
    first.resolve()
    await waitFor(() => expect(screen.getByRole('button', { name: '保存凭证 飞书' })).toBeEnabled())
    expect(feishu.getByLabelText('飞书 App Secret')).toHaveValue('secret-b')
  })

  it('leaves credentials in place when clearing is cancelled or the keyring rejects it', async () => {
    const user = userEvent.setup()
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'error', message: 'missing secret', credentialsConfigured: true }),
    ])
    renderIm()
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    expect(await feishu.findByText('凭证已配置')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: '清除凭证 飞书' }))
    expect(clearImCredentials).not.toHaveBeenCalled()
    expect(feishu.getByText('凭证已配置')).toBeInTheDocument()

    window.confirm = vi.fn(() => true)
    vi.mocked(clearImCredentials).mockRejectedValue(new Error('clear failed'))
    await user.click(screen.getByRole('button', { name: '清除凭证 飞书' }))
    expect(await screen.findByText('clear failed')).toBeInTheDocument()
    expect(screen.queryByText('凭证已清除')).toBeNull()
    expect(feishu.getByText('凭证已配置')).toBeInTheDocument()

    vi.mocked(clearImCredentials).mockResolvedValue(undefined)
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'error', message: 'missing secret', credentialsConfigured: false }),
    ])
    await user.click(screen.getByRole('button', { name: '清除凭证 飞书' }))
    expect(await screen.findByText('凭证未配置')).toBeInTheDocument()
    expect(screen.getByText('凭证已清除')).toBeInTheDocument()
    expect(screen.queryByText('凭证已配置')).toBeNull()
  })

  it('shows connected, retrying, and error states, reconnects, and copies the callback url', async () => {
    const user = userEvent.setup()
    const writeText = vi.fn().mockResolvedValue(undefined)
    vi.spyOn(navigator.clipboard, 'writeText').mockImplementation(writeText)
    const config = backendConfig()
    config.wecom.enabled = true
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'connected', message: 'socket-up', credentialsConfigured: true, lastMessageAt: 1_700_000_000_000 }),
      status({ platform: 'wecom', state: 'retrying', message: 'backoff', credentialsConfigured: false }),
      status({ platform: 'wecom_callback', state: 'error', message: 'callback down', credentialsConfigured: true, webhookUrl: 'http://127.0.0.1:8645/wecom/callback' }),
    ])
    renderIm(config)
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    const wecom = within(screen.getByRole('region', { name: '企业微信机器人' }))
    const callback = within(screen.getByRole('region', { name: '企业微信自建应用' }))
    expect(await feishu.findByRole('status')).toHaveTextContent('已连接')
    expect(wecom.getByRole('alert')).toHaveTextContent('已启用但未配置凭证')
    expect(feishu.getByText(/2023/)).toBeInTheDocument()
    expect(wecom.getByRole('status')).toHaveTextContent('重连中')
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
  })

  it('ignores a stale status response after a newer refresh', async () => {
    const first = deferred<ImStatus[]>()
    vi.mocked(getImStatus)
      .mockImplementationOnce(() => first.promise)
      .mockResolvedValueOnce([status({ platform: 'feishu', state: 'connected', message: 'socket-up', credentialsConfigured: true })])
    renderIm()
    await waitFor(() => expect(statusListener).toEqual(expect.any(Function)))
    statusListener?.()
    expect(await screen.findByText(/socket-up/)).toBeInTheDocument()
    first.resolve([status({ platform: 'feishu', state: 'error', message: 'stale-broker', credentialsConfigured: false })])
    await act(async () => { await Promise.resolve() })
    expect(screen.queryByText(/stale-broker/)).toBeNull()
    expect(screen.getByText(/socket-up/)).toBeInTheDocument()
  })

  it('approves, denies, and revokes pairing, and keeps the row when the action fails', async () => {
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
    expect(screen.queryByText(/OLD/)).toBeNull()
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
  })
})

describe('ImTab setup session', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'] })
    statusListener = null
    pairingListener = null
    unlistenStatus = vi.fn()
    unlistenPairing = vi.fn()
    vi.clearAllMocks()
    window.confirm = vi.fn(() => false)
    vi.mocked(getImStatus).mockResolvedValue([])
    vi.mocked(cancelImSetup).mockResolvedValue(undefined)
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
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })

  async function settle() {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0)
    })
  }

  it('shows a poll failure and leaves the draft unchanged', async () => {
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockRejectedValue(new Error('poll broke'))
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码设置飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(screen.getByText('poll broke')).toBeInTheDocument()
    expect(view.config().feishu.appId).toBe('cli_existing')
    expect(screen.queryByText('扫码完成')).toBeNull()
  })

  it('applies the completed identity onto the draft and keeps credentials off the draft', async () => {
    const session = setupSession()
    vi.mocked(beginImSetup).mockResolvedValue(session)
    vi.mocked(pollImSetup).mockResolvedValue(setupSession({
      status: 'completed',
      message: 'ready',
      identity: { appId: 'cli_from_scan', botId: 'ignored-bot', domain: 'lark', ownerId: 'ou_owner', botName: 'Desk Bot' },
    }))
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码设置飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(view.config().feishu.appId).toBe('cli_from_scan')
    expect(view.config().feishu.domain).toBe('lark')
    expect(view.config().feishu.enabled).toBe(false)
    expect(view.config().wecom.botId).toBe('bot_existing')
    expect(JSON.stringify(view.config())).not.toContain('secret')
    expect(screen.getByText(/Desk Bot/)).toBeInTheDocument()
    expect(screen.getByText(/ou_owner/)).toBeInTheDocument()
  })

  it('ignores a completed poll that arrives after cancel', async () => {
    const session = setupSession()
    const polled = deferred<ImSetupSession>()
    vi.mocked(beginImSetup).mockResolvedValue(session)
    vi.mocked(pollImSetup).mockImplementation(() => polled.promise)
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码设置飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    expect(pollImSetup).toHaveBeenCalledWith('setup-1')
    fireEvent.click(screen.getByRole('switch', { name: '启用飞书' }))
    expect(view.config().feishu.enabled).toBe(true)
    fireEvent.click(screen.getByRole('button', { name: '取消扫码' }))
    await settle()
    polled.resolve(setupSession({
      status: 'completed',
      identity: { appId: 'cli_late', botId: '', domain: 'lark', ownerId: '', botName: 'Late' },
    }))
    await settle()
    expect(view.config().feishu.appId).toBe('cli_existing')
    expect(view.config().feishu.domain).toBe('feishu')
    expect(cancelImSetup).toHaveBeenCalledWith('setup-1')
    expect(screen.queryByText(/cli_late/)).toBeNull()
  })

  it('cancels a setup that is still starting when the page unmounts', async () => {
    const started = deferred<ImSetupSession>()
    vi.mocked(beginImSetup).mockImplementation(() => started.promise)
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码设置飞书' }))
    view.unmount()
    started.resolve(setupSession())
    await settle()
    expect(cancelImSetup).toHaveBeenCalledWith('setup-1')
  })

  it('times out a pending scan and does not apply an identity', async () => {
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockResolvedValue(setupSession({ expiresAt: 1, message: 'still waiting' }))
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码设置飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(screen.getByText(/扫码已超时/)).toBeInTheDocument()
    expect(cancelImSetup).toHaveBeenCalledWith('setup-1')
    expect(view.config().feishu.appId).toBe('cli_existing')
  })

  it('shows a begin failure and does not render a QR code', async () => {
    vi.mocked(beginImSetup).mockRejectedValue(new Error('setup unavailable'))
    renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码设置飞书' }))
    await settle()
    expect(screen.getByText('setup unavailable')).toBeInTheDocument()
    expect(screen.queryByRole('img', { name: '授权二维码' })).toBeNull()
  })
})
