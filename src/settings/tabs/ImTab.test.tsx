import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Lang } from '../../components/i18n'
import {
  approveImPairing,
  beginImSetup,
  cancelImSetup,
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

vi.mock('../../api/im', () => ({
  getImStatus: vi.fn(),
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
  saveImCredentials: vi.fn(),
  subscribeImStatus: vi.fn(async () => () => {}),
  subscribeImPairing: vi.fn(async () => () => {}),
}))

vi.mock('./imQr', () => ({
  authorizationQrDataUrl: vi.fn(async () => 'data:image/png;base64,local-qr'),
}))

beforeAll(() => {
  HTMLDialogElement.prototype.showModal ??= function showModal(this: HTMLDialogElement) {
    this.setAttribute('open', '')
  }
  HTMLDialogElement.prototype.close ??= function close(this: HTMLDialogElement) {
    this.removeAttribute('open')
  }
})

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
  let currentLang = lang
  let mounted = true
  const onChange = (next: ImConfig) => {
    current = next
    if (mounted) view.rerender(<ImTab lang={currentLang} config={current} onChange={onChange} onFlush={onFlush} />)
  }
  const view = render(<ImTab lang={currentLang} config={current} onChange={onChange} onFlush={onFlush} />)
  return {
    onChange,
    setLang: (next: Lang) => {
      currentLang = next
      if (mounted) view.rerender(<ImTab lang={currentLang} config={current} onChange={onChange} onFlush={onFlush} />)
    },
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

  it('shows backend connection states, retries failed connections, and disconnects', async () => {
    const user = userEvent.setup()
    const config = backendConfig()
    config.feishu.enabled = true
    config.wecom.enabled = true
    const onFlush = vi.fn(async () => true)
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'connected', message: 'socket-up', credentialsConfigured: true, lastMessageAt: 1_700_000_000_000 }),
      status({ platform: 'wecom', state: 'error', message: 'connection failed', credentialsConfigured: true }),
    ])
    const view = renderIm(config, 'zh', onFlush)
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    const wecom = within(screen.getByRole('region', { name: '企业微信机器人' }))
    expect(await feishu.findByRole('status')).toHaveTextContent('已连接')
    vi.mocked(reconnectIm).mockRejectedValue(new Error('reconnect refused'))
    await user.click(wecom.getByRole('button', { name: '重新连接 企业微信' }))
    expect(await screen.findByText('reconnect refused')).toBeInTheDocument()
    await user.click(feishu.getByRole('button', { name: '断开连接 飞书' }))
    await waitFor(() => expect(view.config().feishu.enabled).toBe(false))
    expect(onFlush).toHaveBeenCalled()
    expect(within(screen.getByRole('region', { name: '飞书 / Lark' })).queryByRole('button', { name: '断开连接 飞书' })).toBeNull()
  })

  it('restores the platform when disabling cannot be saved', async () => {
    const user = userEvent.setup()
    const config = backendConfig()
    config.feishu.enabled = true
    const view = renderIm(config, 'zh', async () => false)
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    await user.click(feishu.getByRole('button', { name: '断开连接 飞书' }))
    expect(await screen.findByRole('alert')).toBeVisible()
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
    expect(await feishu.findByText('已连接')).toBeInTheDocument()
    first.resolve([status({ platform: 'feishu', state: 'error', message: 'stale-broker', credentialsConfigured: false })])
    await act(async () => { await Promise.resolve() })
    expect(screen.queryByText(/stale-broker/)).toBeNull()
    expect(feishu.getByRole('status')).toHaveTextContent('已连接')
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
    expect(screen.getByRole('alert')).toBeVisible()
    await act(async () => { await Promise.resolve() })
    expect(getImStatus).not.toHaveBeenCalled()
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

  it('closes during QR generation, restores focus, and rejects a late session without blocking the next platform', async () => {
    const started = deferred<ImSetupSession>()
    vi.mocked(beginImSetup)
      .mockImplementationOnce(() => started.promise)
      .mockResolvedValueOnce(setupSession({
        id: 'setup-wecom', platform: 'wecom', url: 'https://work.weixin.qq.com/auth?code=1',
      }))
    renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    const dialog = screen.getByRole('dialog', { name: '飞书 / Lark' })
    expect(within(dialog).getByRole('heading', { name: '飞书 / Lark' })).toHaveFocus()
    expect(screen.getByRole('region', { name: '企业微信机器人' })).toBeVisible()
    fireEvent.keyDown(dialog, { key: 'Escape' })
    fireEvent(dialog, new Event('cancel', { cancelable: true }))
    await settle()
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(screen.getByRole('button', { name: '扫码连接 飞书' })).toHaveFocus()
    expect(screen.getByRole('button', { name: '扫码连接 企业微信' })).toBeEnabled()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 企业微信' }))
    await settle()
    started.resolve(setupSession())
    await settle()
    expect(cancelImSetup).toHaveBeenCalledWith('setup-1')
    expect(commitImSetup).not.toHaveBeenCalled()
    expect(within(screen.getByRole('dialog', { name: '企业微信机器人' })).getByRole('heading', { name: '企业微信机器人' })).toHaveFocus()
    expect(screen.getByRole('img', { name: '授权二维码' })).toBeInTheDocument()
  })

  it('replaces a valid authorization QR with an error when the next link is insecure', async () => {
    vi.mocked(beginImSetup).mockResolvedValueOnce(setupSession())
    renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    expect(screen.getByRole('img', { name: '授权二维码' })).toBeInTheDocument()
    expect(screen.getByText(/打开飞书或 Lark/)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '关闭扫码窗口' }))
    vi.mocked(beginImSetup).mockResolvedValueOnce(setupSession({ id: 'setup-http', platform: 'wecom', url: 'http://accounts.feishu.cn/oauth' }))
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
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(feishu.getByRole('button', { name: '扫码连接 飞书' })).toHaveFocus()
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
    act(() => view.onChange({
      ...view.config(),
      wecom: { ...view.config().wecom, botId: 'bot_later' },
      agent: { ...view.config().agent, workingDirectory: '/later' },
      feishu: { ...view.config().feishu, homeChannel: 'oc_later' },
    }))
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
    act(() => view.onChange({
      ...view.config(), wecom: { ...view.config().wecom, botId: 'bot_later' },
    }))
    fireEvent.click(screen.getByRole('button', { name: '关闭扫码窗口' }))
    commit.resolve(setupSession({ status: 'cancelled', identity: identity() }))
    await settle()
    expect(view.config().feishu).toMatchObject({ appId: 'cli_existing', enabled: true, connectionMode: 'websocket' })
    expect(view.config().wecom.botId).toBe('bot_later')
    expect(screen.queryByText('已连接')).toBeNull()
  })

  it.each(['button', 'escape'] as const)('does not enable a late committed scan after explicit %s cancellation or disturb the next platform', async (control) => {
    const committed = deferred<ImSetupSession>()
    const saved: ImConfig[] = []
    const view = renderIm(backendConfig(), 'zh', async () => {
      saved.push(structuredClone(view.config()))
      return true
    })
    vi.mocked(beginImSetup)
      .mockResolvedValueOnce(setupSession())
      .mockResolvedValueOnce(setupSession({
        id: 'setup-wecom', platform: 'wecom', url: 'https://work.weixin.qq.com/auth?code=1',
      }))
    vi.mocked(pollImSetup).mockResolvedValue(withStatus('authorized', { identity: identity() }))
    vi.mocked(commitImSetup).mockImplementationOnce(() => committed.promise)
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(view.config().feishu).toMatchObject({ appId: 'cli_from_scan', enabled: false })
    if (control === 'button') {
      fireEvent.click(screen.getByRole('button', { name: '关闭扫码窗口' }))
    } else {
      fireEvent(screen.getByRole('dialog'), new Event('cancel', { cancelable: true }))
    }
    await settle()
    expect(screen.queryByRole('dialog')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 企业微信' }))
    await settle()
    committed.resolve(setupSession({ status: 'completed', identity: identity() }))
    await settle()
    expect(view.config().feishu).toMatchObject({ appId: 'cli_from_scan', enabled: false })
    expect(saved.at(-1)?.feishu).toMatchObject({ appId: 'cli_from_scan', enabled: false })
    expect(view.config().wecom).toMatchObject({ botId: 'bot_existing', enabled: false })
    expect(within(screen.getByRole('dialog', { name: '企业微信机器人' })).getByRole('img', { name: '授权二维码' })).toBeVisible()
  })

  it('keeps an active scan working when the language changes', async () => {
    const view = renderIm()
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockResolvedValue(withStatus('authorized', { identity: identity() }))
    vi.mocked(commitImSetup).mockResolvedValue(setupSession({ status: 'completed', identity: identity() }))
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    act(() => view.setLang('en'))
    await settle()
    expect(within(screen.getByRole('dialog')).getByRole('img')).toBeVisible()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(view.config().feishu).toMatchObject({ appId: 'cli_from_scan', enabled: true })
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('closes after persistence without waiting for status and reports a background refresh failure without undoing authorization', async () => {
    const refreshed = deferred<ImStatus[]>()
    const saved: ImConfig[] = []
    vi.mocked(getImStatus).mockResolvedValueOnce([]).mockImplementationOnce(() => refreshed.promise)
    const view = renderIm(backendConfig(), 'zh', async () => {
      saved.push(structuredClone(view.config()))
      return true
    })
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockResolvedValue(withStatus('authorized', { identity: identity() }))
    vi.mocked(commitImSetup).mockResolvedValue(setupSession({ status: 'completed', identity: identity() }))
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(saved.at(-1)?.feishu).toMatchObject({ appId: 'cli_from_scan', enabled: true })
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(screen.getByRole('button', { name: '扫码连接 飞书' })).toHaveFocus()
    refreshed.reject(new Error('status unavailable'))
    await settle()
    expect(screen.getByRole('alert')).toHaveTextContent('status unavailable')
    expect(view.config().feishu).toMatchObject({ appId: 'cli_from_scan', enabled: true })
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
    act(() => view.onChange({
      ...view.config(), feishu: { ...view.config().feishu, appId: 'cli_typed' },
    }))
    flushed.resolve(true)
    await settle()
    expect(commitImSetup).not.toHaveBeenCalled()
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
    fireEvent.click(screen.getByRole('button', { name: '关闭扫码窗口' }))
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 企业微信' }))
    await settle()
    expect(cancelImSetup).toHaveBeenCalledWith('setup-1')
    late.resolve(withStatus('authorized', { identity: identity() }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(commitImSetup).not.toHaveBeenCalled()
    expect(view.config().feishu.appId).toBe('cli_existing')
    expect(within(screen.getByRole('dialog', { name: '企业微信机器人' })).getByText(/扫码被拒绝/)).toBeInTheDocument()
  })

  it('ignores a completed poll that arrives after cancel', async () => {
    const polled = deferred<ImSetupSession>()
    vi.mocked(beginImSetup).mockResolvedValue(setupSession())
    vi.mocked(pollImSetup).mockImplementation(() => polled.promise)
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    fireEvent.click(screen.getByRole('button', { name: '关闭扫码窗口' }))
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
    fireEvent.click(screen.getByRole('button', { name: '重新扫码 飞书' }))
    await settle()
    await act(async () => { await vi.advanceTimersByTimeAsync(IM_SETUP_POLL_MS) })
    await settle()
    expect(screen.getByText('poll broke')).toBeInTheDocument()
    expect(view.config().feishu.appId).toBe('cli_existing')

    vi.mocked(pollImSetup).mockResolvedValueOnce(setupSession({ status: 'completed', identity: identity() }))
    fireEvent.click(screen.getByRole('button', { name: '重新扫码 飞书' }))
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

describe('ImTab group access and manual connection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    installListeners()
    vi.mocked(saveImCredentials).mockResolvedValue(undefined)
  })

  it('saves allowed groups and per-group users while keeping the other settings', async () => {
    const view = renderIm()
    const initial = view.config()
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    expect(feishu.queryByRole('region', { name: '开放访问说明' })).toBeNull()
    fireEvent.change(feishu.getByRole('textbox', { name: '飞书允许的群' }), { target: { value: 'oc_new, oc_two' } })
    expect(feishu.queryByRole('textbox', { name: '飞书群内用户 oc_old' })).toBeNull()
    fireEvent.change(feishu.getByRole('textbox', { name: '飞书群内用户 oc_new' }), { target: { value: 'ou_a, ou_b' } })
    fireEvent.change(feishu.getByRole('textbox', { name: '飞书群内用户 oc_two' }), { target: { value: 'ou_c' } })
    fireEvent.click(feishu.getByRole('button', { name: '保存群访问 飞书' }))
    await waitFor(() => expect(view.config().feishu.access.allowedGroups).toEqual(['oc_new', 'oc_two']))
    expect(view.config().feishu.access).toEqual({
      dmPolicy: 'pairing',
      groupPolicy: 'allowlist',
      allowedUsers: ['ou_old'],
      allowedGroups: ['oc_new', 'oc_two'],
      groupUsers: { oc_new: ['ou_a', 'ou_b'], oc_two: ['ou_c'] },
    })
    expect(view.config().feishu.webhook).toEqual(initial.feishu.webhook)
    expect(view.config().feishu.homeChannel).toBe(initial.feishu.homeChannel)
    expect(view.config().feishu.requireMention).toBe(true)
    expect(view.config().wecom).toEqual(initial.wecom)
    expect(view.config().wecomCallback).toEqual(initial.wecomCallback)
    expect(view.config().agent).toEqual(initial.agent)

    const wecom = within(screen.getByRole('region', { name: '企业微信机器人' }))
    fireEvent.change(wecom.getByRole('textbox', { name: '企业微信允许的群' }), { target: { value: 'wr_group' } })
    fireEvent.change(wecom.getByRole('textbox', { name: '企业微信群内用户 wr_group' }), { target: { value: 'zhang' } })
    fireEvent.click(wecom.getByRole('button', { name: '保存群访问 企业微信' }))
    await waitFor(() => expect(view.config().wecom.access.allowedGroups).toEqual(['wr_group']))
    expect(view.config().wecom.access.groupUsers).toEqual({ wr_group: ['zhang'] })
    expect(view.config().wecom.access.dmPolicy).toBe('pairing')
    expect(view.config().wecom.access.groupPolicy).toBe('allowlist')
    expect(view.config().feishu.access.allowedGroups).toEqual(['oc_new', 'oc_two'])
    expect(view.config().agent).toEqual(initial.agent)
  })

  it('shows the failed group save and explains open or wildcard access without applying it', async () => {
    const view = renderIm(backendConfig(), 'zh', async () => false)
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    fireEvent.change(feishu.getByRole('textbox', { name: '飞书允许的群' }), { target: { value: 'oc_failed' } })
    fireEvent.click(feishu.getByRole('button', { name: '保存群访问 飞书' }))
    await waitFor(() => expect(feishu.getByRole('alert')).toBeVisible())
    expect(view.config().feishu.access.allowedGroups).toEqual(['oc_old'])
    expect(view.config().feishu.access.groupUsers).toEqual({ oc_old: ['ou_old'] })
    expect(view.config().wecom).toEqual(backendConfig().wecom)
    fireEvent.click(feishu.getByRole('button', { name: '飞书群策略' }))
    fireEvent.click(screen.getByRole('option', { name: '开放' }))
    expect(feishu.getByRole('region', { name: '开放访问说明' })).toHaveTextContent(/本机工具/)
    expect(view.config().feishu.access.groupPolicy).toBe('allowlist')
    fireEvent.click(feishu.getByRole('button', { name: '飞书群策略' }))
    fireEvent.click(screen.getByRole('option', { name: '允许列表' }))
    fireEvent.change(feishu.getByRole('textbox', { name: '飞书允许的群' }), { target: { value: '*' } })
    expect(feishu.getByRole('region', { name: '开放访问说明' })).toBeVisible()
    expect(view.config().feishu.access.allowedGroups).toEqual(['oc_old'])
  })

  it('saves WeCom group access from the English page', async () => {
    const view = renderIm(backendConfig(), 'en')
    const wecom = within(screen.getByRole('region', { name: 'WeCom bot' }))
    expect(wecom.getByRole('button', { name: 'Manual setup WeCom' })).toBeVisible()
    fireEvent.change(wecom.getByRole('textbox', { name: 'WeCom allowed groups' }), { target: { value: 'wr_en' } })
    fireEvent.change(wecom.getByRole('textbox', { name: 'WeCom users in wr_en' }), { target: { value: 'alice' } })
    fireEvent.click(wecom.getByRole('button', { name: 'Save group access WeCom' }))
    await waitFor(() => expect(view.config().wecom.access.allowedGroups).toEqual(['wr_en']))
    expect(view.config().wecom.access.groupUsers).toEqual({ wr_en: ['alice'] })
    expect(view.config().wecom.access.dmPolicy).toBe('pairing')
    expect(view.config().feishu.appId).toBe('cli_existing')
    fireEvent.click(wecom.getByRole('button', { name: 'WeCom group policy' }))
    fireEvent.click(screen.getByRole('option', { name: 'Open' }))
    expect(wecom.getByRole('region', { name: 'Open access warning' })).toHaveTextContent(/local tools/)
    expect(view.config().wecom.access.groupPolicy).toBe('allowlist')
  })

  it('does not store a secret when the manual identity cannot be saved', async () => {
    const view = renderIm(backendConfig(), 'zh', async () => false)
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    fireEvent.click(feishu.getByRole('button', { name: '手动配置 飞书' }))
    expect(feishu.getByLabelText('密钥')).toHaveValue('')
    fireEvent.change(feishu.getByRole('textbox', { name: 'App ID' }), { target: { value: 'cli_manual' } })
    fireEvent.change(feishu.getByLabelText('密钥'), { target: { value: 'secret-value' } })
    fireEvent.click(feishu.getByRole('button', { name: '保存并连接 飞书' }))
    await waitFor(() => expect(feishu.getByRole('alert')).toBeVisible())
    expect(saveImCredentials).not.toHaveBeenCalled()
    expect(view.config().feishu.appId).toBe('cli_existing')
    expect(view.config().feishu.enabled).toBe(false)
    expect(JSON.stringify(view.config())).not.toContain('secret-value')
    expect(view.config().wecom.botId).toBe('bot_existing')
  })

  it('does not enable the platform when the secret cannot be stored and restores the previous bot', async () => {
    const initial = backendConfig()
    initial.feishu.enabled = true
    initial.feishu.connectionMode = 'webhook'
    const view = renderIm(initial)
    vi.mocked(saveImCredentials).mockRejectedValue(new Error('credential write failed'))
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    fireEvent.click(feishu.getByRole('button', { name: '手动配置 飞书' }))
    fireEvent.click(feishu.getByRole('button', { name: '飞书域' }))
    fireEvent.click(screen.getByRole('option', { name: 'Lark' }))
    fireEvent.change(feishu.getByRole('textbox', { name: 'App ID' }), { target: { value: 'cli_manual' } })
    fireEvent.change(feishu.getByLabelText('密钥'), { target: { value: 'secret-value' } })
    fireEvent.click(feishu.getByRole('button', { name: '保存并连接 飞书' }))
    await waitFor(() => expect(feishu.getByRole('alert')).toBeVisible())
    expect(saveImCredentials).toHaveBeenCalledTimes(1)
    expect(view.config().feishu).toMatchObject({
      appId: 'cli_existing',
      enabled: true,
      connectionMode: 'webhook',
      domain: 'feishu',
    })
    expect(view.config().feishu.webhook.path).toBe('/feishu/webhook')
    expect(view.config().feishu.access.allowedGroups).toEqual(['oc_old'])
    expect(view.config().wecom.botId).toBe('bot_existing')
    expect(JSON.stringify(view.config())).not.toContain('secret-value')
    expect(feishu.getByRole('status')).not.toHaveTextContent('已连接')
  })

  it('enables a saved manual bot and keeps the connection label from backend status', async () => {
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'feishu', state: 'retrying', message: 'opening', credentialsConfigured: true }),
    ])
    const view = renderIm()
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    fireEvent.click(feishu.getByRole('button', { name: '手动配置 飞书' }))
    fireEvent.change(feishu.getByRole('textbox', { name: 'App ID' }), { target: { value: 'cli_manual' } })
    fireEvent.change(feishu.getByLabelText('密钥'), { target: { value: 'secret-value' } })
    fireEvent.click(feishu.getByRole('button', { name: '保存并连接 飞书' }))
    await waitFor(() => expect(view.config().feishu.enabled).toBe(true))
    expect(view.config().feishu.appId).toBe('cli_manual')
    expect(view.config().feishu.domain).toBe('feishu')
    expect(view.config().feishu.connectionMode).toBe('websocket')
    expect(view.config().feishu.webhook.path).toBe('/feishu/webhook')
    expect(view.config().wecom.botId).toBe('bot_existing')
    expect(saveImCredentials).toHaveBeenCalledWith('feishu', 'cli_manual', expect.objectContaining({
      secret: 'secret-value', encryptKey: '', verificationToken: '', token: '', encodingAesKey: '',
    }))
    expect(JSON.stringify(view.config())).not.toContain('secret-value')
    expect(feishu.getByLabelText('密钥')).toHaveValue('')
    expect(feishu.getByRole('status')).toHaveTextContent('重连中')
    expect(screen.queryByText('已连接')).toBeNull()
  })

  it('connects WeCom manually without changing Feishu or storing the secret in settings', async () => {
    vi.mocked(getImStatus).mockResolvedValue([
      status({ platform: 'wecom', state: 'connecting', message: 'opening', credentialsConfigured: true }),
    ])
    const view = renderIm()
    const wecom = within(screen.getByRole('region', { name: '企业微信机器人' }))
    fireEvent.click(wecom.getByRole('button', { name: '手动配置 企业微信' }))
    expect(wecom.getByLabelText('密钥')).toHaveValue('')
    fireEvent.change(wecom.getByRole('textbox', { name: 'Bot ID' }), { target: { value: 'bot_manual' } })
    fireEvent.change(wecom.getByLabelText('密钥'), { target: { value: 'wecom-secret' } })
    fireEvent.click(wecom.getByRole('button', { name: '保存并连接 企业微信' }))
    await waitFor(() => expect(view.config().wecom.enabled).toBe(true))
    expect(view.config().wecom.botId).toBe('bot_manual')
    expect(view.config().wecom.websocketUrl).toBe('wss://openws.work.weixin.qq.com')
    expect(view.config().wecom.access.groupPolicy).toBe('allowlist')
    expect(view.config().feishu).toMatchObject({ appId: 'cli_existing', enabled: false, domain: 'feishu' })
    expect(JSON.stringify(view.config())).not.toContain('wecom-secret')
    expect(wecom.getByRole('status')).toHaveTextContent('连接中')
    expect(saveImCredentials).toHaveBeenCalledWith('wecom', 'bot_manual', expect.objectContaining({ secret: 'wecom-secret' }))
  })

  it('does not store a secret when manual connect is cancelled before the identity flush finishes', async () => {
    const flushed = deferred<boolean>()
    const view = renderIm(backendConfig(), 'zh', () => flushed.promise)
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    fireEvent.click(feishu.getByRole('button', { name: '手动配置 飞书' }))
    fireEvent.change(feishu.getByRole('textbox', { name: 'App ID' }), { target: { value: 'cli_manual' } })
    fireEvent.change(feishu.getByLabelText('密钥'), { target: { value: 'secret-value' } })
    fireEvent.click(feishu.getByRole('button', { name: '保存并连接 飞书' }))
    await waitFor(() => expect(view.config().feishu.appId).toBe('cli_manual'))
    expect(view.config().feishu.enabled).toBe(false)
    fireEvent.click(feishu.getByRole('button', { name: '取消手动连接 飞书' }))
    flushed.resolve(true)
    await waitFor(() => expect(view.config().feishu.appId).toBe('cli_existing'))
    expect(saveImCredentials).not.toHaveBeenCalled()
    expect(view.config().feishu.enabled).toBe(false)
    expect(JSON.stringify(view.config())).not.toContain('secret-value')
  })

  it('does not enable a manual connection cancelled before the secret or the enable flush returns', async () => {
    const secretCall = deferred<void>()
    vi.mocked(saveImCredentials).mockImplementation(() => secretCall.promise)
    const view = renderIm()
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    fireEvent.click(feishu.getByRole('button', { name: '手动配置 飞书' }))
    fireEvent.change(feishu.getByRole('textbox', { name: 'App ID' }), { target: { value: 'cli_manual' } })
    fireEvent.change(feishu.getByLabelText('密钥'), { target: { value: 'secret-value' } })
    fireEvent.click(feishu.getByRole('button', { name: '保存并连接 飞书' }))
    await waitFor(() => expect(saveImCredentials).toHaveBeenCalledTimes(1))
    expect(view.config().feishu).toMatchObject({ appId: 'cli_manual', enabled: false })
    fireEvent.click(feishu.getByRole('button', { name: '取消手动连接 飞书' }))
    secretCall.resolve()
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(view.config().feishu.enabled).toBe(false)
    view.unmount()

    const enableFlush = deferred<boolean>()
    let flushes = 0
    vi.mocked(saveImCredentials).mockResolvedValue(undefined)
    const late = renderIm(backendConfig(), 'zh', () => {
      flushes += 1
      return flushes === 1 ? Promise.resolve(true) : enableFlush.promise
    })
    const lateFeishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    vi.mocked(saveImCredentials).mockResolvedValue(undefined)
    fireEvent.click(lateFeishu.getByRole('button', { name: '手动配置 飞书' }))
    fireEvent.change(lateFeishu.getByRole('textbox', { name: 'App ID' }), { target: { value: 'cli_late' } })
    fireEvent.change(lateFeishu.getByLabelText('密钥'), { target: { value: 'late-secret' } })
    fireEvent.click(lateFeishu.getByRole('button', { name: '保存并连接 飞书' }))
    await waitFor(() => expect(late.config().feishu.enabled).toBe(true))
    fireEvent.click(lateFeishu.getByRole('button', { name: '取消手动连接 飞书' }))
    enableFlush.resolve(true)
    await waitFor(() => expect(late.config().feishu.enabled).toBe(false))
    expect(late.config().feishu.appId).toBe('cli_late')
    expect(JSON.stringify(late.config())).not.toContain('late-secret')
  })

  it('does not let a cancelled manual save roll back the next attempt or store the first secret', async () => {
    const identityFlush = deferred<boolean>()
    const secretCall = deferred<void>()
    let flushes = 0
    vi.mocked(saveImCredentials).mockImplementation(() => secretCall.promise)
    const view = renderIm(backendConfig(), 'zh', () => ++flushes === 1 ? identityFlush.promise : Promise.resolve(true))
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    fireEvent.click(feishu.getByRole('button', { name: '手动配置 飞书' }))
    fireEvent.change(feishu.getByLabelText('App ID'), { target: { value: 'cli_manual' } })
    fireEvent.change(feishu.getByLabelText('密钥'), { target: { value: 'first-secret' } })
    fireEvent.click(feishu.getByRole('button', { name: '保存并连接 飞书' }))
    await waitFor(() => expect(view.config().feishu.appId).toBe('cli_manual'))
    fireEvent.click(feishu.getByRole('button', { name: '取消手动连接 飞书' }))
    const connect = feishu.getByRole('button', { name: /保存并连接/ })
    expect(connect).toBeDisabled()
    const scan = feishu.getByRole('button', { name: '扫码连接 飞书' })
    expect(scan).toBeDisabled()
    fireEvent.click(scan)
    expect(beginImSetup).not.toHaveBeenCalled()
    fireEvent.change(feishu.getByLabelText('App ID'), { target: { value: 'cli_next' } })
    fireEvent.change(feishu.getByLabelText('密钥'), { target: { value: 'second-secret' } })
    fireEvent.click(connect)
    expect(view.config().feishu.appId).toBe('cli_manual')
    expect(saveImCredentials).not.toHaveBeenCalled()
    identityFlush.resolve(true)
    await waitFor(() => expect(view.config().feishu.appId).toBe('cli_existing'))
    expect(feishu.getByRole('button', { name: '保存并连接 飞书' })).toBeEnabled()
    expect(scan).toBeEnabled()
    expect(saveImCredentials).not.toHaveBeenCalled()
    fireEvent.change(feishu.getByLabelText('App ID'), { target: { value: 'cli_next' } })
    fireEvent.change(feishu.getByLabelText('密钥'), { target: { value: 'second-secret' } })
    fireEvent.click(feishu.getByRole('button', { name: '保存并连接 飞书' }))
    await waitFor(() => expect(saveImCredentials).toHaveBeenCalledTimes(1))
    expect(view.config().feishu).toMatchObject({ appId: 'cli_next', enabled: false })
    expect(saveImCredentials).toHaveBeenCalledWith('feishu', 'cli_next', expect.objectContaining({ secret: 'second-secret' }))
    secretCall.resolve()
    await waitFor(() => expect(view.config().feishu.enabled).toBe(true))
    expect(view.config().feishu.appId).toBe('cli_next')
    expect(saveImCredentials).toHaveBeenCalledTimes(1)
    expect(JSON.stringify(view.config())).not.toContain('first-secret')
    expect(JSON.stringify(view.config())).not.toContain('second-secret')
  })

  it('does not enable a manual bot when the page unmounts during the credential write', async () => {
    const secretCall = deferred<void>()
    vi.mocked(saveImCredentials).mockImplementation(() => secretCall.promise)
    const view = renderIm()
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    fireEvent.click(feishu.getByRole('button', { name: '手动配置 飞书' }))
    fireEvent.change(feishu.getByLabelText('App ID'), { target: { value: 'cli_manual' } })
    fireEvent.change(feishu.getByLabelText('密钥'), { target: { value: 'secret-value' } })
    fireEvent.click(feishu.getByRole('button', { name: '保存并连接 飞书' }))
    await waitFor(() => expect(saveImCredentials).toHaveBeenCalledTimes(1))
    view.unmount()
    await act(async () => { secretCall.resolve(); await secretCall.promise })
    expect(view.config().feishu.enabled).toBe(false)
    expect(view.config().feishu.appId).toBe('cli_manual')
    expect(JSON.stringify(view.config())).not.toContain('secret-value')
  })

  it('does not enable a manual bot when the page unmounts during the enable flush', async () => {
    const enableFlush = deferred<boolean>()
    let flushes = 0
    const view = renderIm(backendConfig(), 'zh', () => {
      flushes += 1
      if (flushes === 2) return enableFlush.promise
      return Promise.resolve(true)
    })
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    fireEvent.click(feishu.getByRole('button', { name: '手动配置 飞书' }))
    fireEvent.change(feishu.getByLabelText('App ID'), { target: { value: 'cli_manual' } })
    fireEvent.change(feishu.getByLabelText('密钥'), { target: { value: 'secret-value' } })
    fireEvent.click(feishu.getByRole('button', { name: '保存并连接 飞书' }))
    await waitFor(() => expect(view.config().feishu.enabled).toBe(true))
    view.unmount()
    enableFlush.resolve(true)
    await waitFor(() => expect(view.config().feishu.enabled).toBe(false))
    expect(view.config().feishu.appId).toBe('cli_manual')
    expect(saveImCredentials).toHaveBeenCalledTimes(1)
  })

  it('opens manual setup from a failed scan and uses a chosen Lark domain without writing it early', async () => {
    vi.mocked(beginImSetup).mockRejectedValueOnce(new Error('setup unavailable')).mockResolvedValue(setupSession())
    const view = renderIm()
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await waitFor(() => expect(screen.getByRole('button', { name: '改用手动配置' })).toBeVisible())
    fireEvent.click(screen.getByRole('button', { name: '改用手动配置' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    const feishu = within(screen.getByRole('region', { name: '飞书 / Lark' }))
    expect(feishu.getByRole('textbox', { name: 'App ID' })).toBeVisible()
    fireEvent.click(feishu.getByRole('button', { name: '飞书域' }))
    fireEvent.click(screen.getByRole('option', { name: 'Lark' }))
    expect(view.config().feishu.domain).toBe('feishu')
    fireEvent.click(screen.getByRole('button', { name: '扫码连接 飞书' }))
    await waitFor(() => expect(beginImSetup).toHaveBeenCalledWith('feishu', 'lark'))
    expect(view.config().feishu.domain).toBe('feishu')
    expect(view.config().feishu.appId).toBe('cli_existing')
  })
})
