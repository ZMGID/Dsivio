import { useCallback, useEffect, useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { confirmDialog } from '../../components/dialogQueue'
import { Button } from '../../components/Button'
import type { Lang } from '../../components/i18n'
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
import type {
  ConnectionState, CredentialInput, DmPolicy, FeishuConfig, FeishuConnectionMode, FeishuDomain, GroupPolicy,
  ImAccessConfig, ImAgentConfig, ImApprovedUser, ImConfig, ImPairingRequest, ImPlatform, ImSetupSession,
  ImStatus, ImWebhookConfig, WecomCallbackConfig, WecomConfig,
} from '../../api/im'
import type { ModelProvider } from '../../api/tauri'
import { ModelPairSelect } from '../ModelPairSelect'
import { FieldBlock, Input, Select, SettingRow, SettingsGroup, TextArea, Toggle } from '../components'
import { authorizationQrDataUrl } from './imQr'

export const IM_SETUP_POLL_MS = 1000

const TERMINAL_SETUP: Partial<Record<ImSetupSession['status'], true>> = {
  completed: true, denied: true, expired: true, cancelled: true, error: true,
}

type SecretDraft = CredentialInput

type Copy = ReturnType<typeof imCopy>

function emptySecrets(): SecretDraft {
  return { secret: '', encryptKey: '', verificationToken: '', token: '', encodingAesKey: '' }
}

function unixMs(value: number): number {
  if (!Number.isFinite(value)) return 0
  return value > 0 && value < 1_000_000_000_000 ? value * 1000 : value
}

function sameSecrets(a: SecretDraft, b: SecretDraft): boolean {
  return a.secret === b.secret && a.encryptKey === b.encryptKey && a.verificationToken === b.verificationToken
    && a.token === b.token && a.encodingAesKey === b.encodingAesKey
}

function errorText(error: unknown, fallback: string): string {
  if (typeof error === 'string' && error.trim()) return error
  if (error instanceof Error && error.message.trim()) return error.message
  return fallback
}

function isAbort(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError'
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(new DOMException('aborted', 'AbortError'))
      return
    }
    const timer = setTimeout(resolve, ms)
    signal.addEventListener('abort', () => {
      clearTimeout(timer)
      reject(new DOMException('aborted', 'AbortError'))
    }, { once: true })
  })
}

function parseIdList(value: string): string[] {
  const seen = new Set<string>()
  const ids: string[] = []
  for (const part of value.split(/[\n,]/)) {
    const id = part.trim()
    if (!id || seen.has(id)) continue
    seen.add(id)
    ids.push(id)
  }
  return ids
}

function isHttpUrl(url: string): boolean {
  try {
    const parsed = new URL(url)
    return parsed.protocol === 'https:' || parsed.protocol === 'http:'
  } catch {
    return false
  }
}

function pastExpiry(session: ImSetupSession, now = Date.now()): boolean {
  return unixMs(session.expiresAt) <= now
}

function applyImSetupIdentity(config: ImConfig, session: ImSetupSession): ImConfig {
  const identity = session.identity
  if (!identity || session.status !== 'completed') return config
  if (session.platform === 'feishu') {
    return {
      ...config,
      feishu: {
        ...config.feishu,
        appId: identity.appId.trim() ? identity.appId : config.feishu.appId,
        domain: identity.domain || config.feishu.domain,
      },
    }
  }
  if (session.platform === 'wecom') {
    return {
      ...config,
      wecom: {
        ...config.wecom,
        botId: identity.botId.trim() ? identity.botId : config.wecom.botId,
      },
    }
  }
  return config
}

function imCopy(lang: Lang) {
  const zh = lang === 'zh'
  return {
    zh,
    pageWarnings: [
      zh ? '通过即时通讯调用的所有工具都会自动批准，不再弹出桌面确认。' : 'Every tool invoked from IM is approved automatically. Desktop confirmation is not shown.',
      zh ? 'Dsivio 需要在这台电脑上保持运行，关闭应用后机器人会离线。' : 'Dsivio must stay running on this computer. The bot goes offline when the app quits.',
      zh ? '同一个机器人不要同时连接 Hermes 和 Dsivio，以免抢占连接或分流消息。' : 'Do not connect the same bot to Hermes and Dsivio together: connections may be displaced or messages split between them.',
    ],
    missing: zh ? '后端未返回 IM 配置，此页不会自行补默认值。' : 'The backend did not return IM settings. This page will not invent defaults.',
    agentTitle: zh ? '共享助手' : 'Shared assistant',
    agentHint: zh ? '三个平台共用助手、模型和工作目录。通知频道按平台分别保存。' : 'All three platforms share the assistant, model, and working directory. Each platform keeps its own home channel.',
    assistant: zh ? '助手 ID' : 'Assistant ID',
    model: zh ? '模型' : 'Model',
    workdir: zh ? '工作目录' : 'Working directory',
    workdirHint: zh ? '留空表示沿用后端默认目录。' : 'Empty keeps the backend default directory.',
    chooseDir: zh ? '选择工作目录' : 'Choose working directory',
    clearDir: zh ? '清空工作目录' : 'Clear working directory',
    groupSessions: zh ? '群聊按用户分开会话' : 'Separate group sessions per user',
    streaming: zh ? '流式回复' : 'Streaming replies',
    homeFeishu: zh ? '飞书通知频道' : 'Feishu home channel',
    homeWecom: zh ? '企业微信通知频道' : 'WeCom home channel',
    homeCallback: zh ? '自建应用通知频道' : 'Self-built app home channel',
    feishu: zh ? '飞书 / Lark' : 'Feishu / Lark',
    wecom: zh ? '企业微信机器人' : 'WeCom bot',
    callback: zh ? '企业微信自建应用' : 'WeCom self-built app',
    enabledFeishu: zh ? '启用飞书' : 'Enable Feishu',
    enabledWecom: zh ? '启用企业微信机器人' : 'Enable WeCom bot',
    enabledCallback: zh ? '启用企业微信自建应用' : 'Enable WeCom self-built app',
    appId: zh ? '飞书 App ID' : 'Feishu App ID',
    domain: zh ? '飞书域名' : 'Feishu domain',
    connection: zh ? '飞书连接方式' : 'Feishu connection',
    botId: zh ? '企业微信 Bot ID' : 'WeCom bot ID',
    websocketUrl: zh ? '企业微信 WebSocket 地址' : 'WeCom WebSocket URL',
    corpId: zh ? '企业 ID' : 'Corp ID',
    agentId: zh ? '应用 Agent ID' : 'Agent ID',
    host: zh ? 'Webhook 主机' : 'Webhook host',
    port: zh ? 'Webhook 端口' : 'Webhook port',
    path: zh ? 'Webhook 路径' : 'Webhook path',
    dm: zh ? '私聊策略' : 'Direct-message policy',
    group: zh ? '群策略' : 'Group policy',
    users: zh ? '允许的用户' : 'Allowed users',
    usersHint: zh ? '每行一个，也接受逗号分隔。这是配置允许列表，不是配对存储。' : 'One per line, or comma-separated. This is the settings allowlist, not the pairing store.',
    groups: zh ? '允许的群' : 'Allowed groups',
    groupsHint: zh ? '每行一个群 ID。' : 'One group ID per line.',
    groupUsers: zh ? '群内用户' : 'Users inside a group',
    addGroupUsers: zh ? '添加群用户规则' : 'Add group-user rule',
    groupId: zh ? '群 ID' : 'Group ID',
    groupUserIds: zh ? '该群允许的用户' : 'Users allowed in this group',
    removeGroup: zh ? '移除群用户规则' : 'Remove group-user rule',
    mention: zh ? '群消息需要 @ 机器人' : 'Require @ in groups',
    secretFeishu: zh ? '飞书 App Secret' : 'Feishu App Secret',
    encryptKey: zh ? '飞书 Encrypt Key' : 'Feishu Encrypt Key',
    verification: zh ? '飞书 Verification Token' : 'Feishu Verification Token',
    secretWecom: zh ? '企业微信机器人 Secret' : 'WeCom bot secret',
    secretCallback: zh ? '自建应用 Secret' : 'Self-built app secret',
    callbackToken: zh ? '回调 Token' : 'Callback token',
    aes: zh ? 'EncodingAESKey' : 'EncodingAESKey',
    save: zh ? '保存凭证' : 'Save credentials',
    clear: zh ? '清除凭证' : 'Clear credentials',
    clearConfirm: zh ? '清除后机器人会因缺少凭证而连接失败。确定清除已保存的凭证？' : 'The bot will fail to connect without credentials. Clear the saved credentials?',
    credentialHint: zh ? '密钥按已保存的机器人 ID 写入系统钥匙串。保存凭证会先把设置写入后端，成功后才提交密钥。密钥不会进入设置草稿，也不会从状态接口读回。' : 'Secrets are stored in the OS keyring for the saved bot ID. Saving credentials writes settings first, then submits the secret only if that succeeds. Secrets are not part of the settings draft and are not returned by status.',
    flushFailed: zh ? '设置没有保存，凭证没有写入。' : 'Settings were not saved, so the credentials were not stored.',
    identityRequired: zh ? '请先填写机器人 ID。凭证按这个 ID 保存。' : 'Enter the bot ID first. Credentials are stored for that ID.',
    boundId: zh ? '绑定 ID' : 'Bound ID',
    configured: zh ? '凭证已配置' : 'Credentials configured',
    notConfigured: zh ? '凭证未配置' : 'Credentials not configured',
    enabledMissing: zh ? '已启用但未配置凭证，连接会失败。' : 'Enabled without credentials. The connection will fail.',
    saveOk: zh ? '凭证已保存' : 'Credentials saved',
    saveKept: zh ? '刚才提交的凭证已保存。输入框里后来改过的内容还没保存。' : 'The credentials you submitted were saved. Later edits in the fields are not saved yet.',
    clearOk: zh ? '凭证已清除' : 'Credentials cleared',
    requestFailed: zh ? '请求失败' : 'Request failed',
    scanFeishu: zh ? '扫码设置飞书' : 'Set up Feishu with QR',
    scanWecom: zh ? '扫码设置企业微信' : 'Set up WeCom with QR',
    setup: zh ? '扫码设置' : 'QR setup',
    qrAlt: zh ? '授权二维码' : 'Authorization QR code',
    cancelSetup: zh ? '取消扫码' : 'Cancel setup',
    missingUrl: zh ? '后端没有返回授权链接。' : 'The backend did not return an authorization link.',
    badUrl: zh ? '授权链接不是 http(s)，已停止展示。' : 'The authorization link is not http(s), so it is not shown.',
    identityMissing: zh ? '扫码已完成，但后端没有返回身份，设置草稿未改。' : 'Setup completed without an identity, so the settings draft was left unchanged.',
    timeout: zh ? '扫码已超时。' : 'QR setup timed out.',
    cancelled: zh ? '扫码已取消。' : 'QR setup was cancelled.',
    owner: zh ? '所有者' : 'Owner',
    botName: zh ? '机器人名称' : 'Bot name',
    reconnect: zh ? '重新连接' : 'Reconnect',
    copyUrl: zh ? '复制回调地址' : 'Copy callback URL',
    copied: zh ? '回调地址已复制' : 'Callback URL copied',
    lastMessage: zh ? '最近一条消息 (UTC)' : 'Last message (UTC)',
    noStatus: zh ? '尚未收到该平台状态。' : 'No status has arrived for this platform.',
    statusFailed: zh ? '状态刷新失败' : 'Status refresh failed',
    retryStatus: zh ? '重试刷新状态' : 'Retry status refresh',
    pairing: zh ? '配对' : 'Pairing',
    pairingHint: zh ? '待处理请求来自配对存储。批准或拒绝后列表以服务端结果为准。已批准用户可撤销。' : 'Pending requests come from the pairing store. After approve or deny, the list shows the server result. Approved users can be revoked.',
    noPending: zh ? '没有待处理的配对请求。' : 'No pending pairing requests.',
    noApproved: zh ? '没有已批准用户。' : 'No approved users.',
    approve: zh ? '批准' : 'Approve',
    deny: zh ? '拒绝' : 'Deny',
    revoke: zh ? '撤销' : 'Revoke',
    pendingTitle: zh ? '待处理请求' : 'Pending requests',
    approvedTitle: zh ? '已批准用户' : 'Approved users',
    refreshFailed: zh ? '操作已提交，但列表刷新失败。' : 'The action was submitted, but refreshing the list failed.',
    pairingFailed: zh ? '配对列表刷新失败' : 'Pairing list refresh failed',
    retryPairing: zh ? '重试刷新配对' : 'Retry pairing refresh',
    dirFailed: zh ? '无法选择目录' : 'Could not choose a directory',
    state: {
      disabled: zh ? '未启用' : 'Disabled',
      connecting: zh ? '连接中' : 'Connecting',
      connected: zh ? '已连接' : 'Connected',
      retrying: zh ? '重连中' : 'Retrying',
      error: zh ? '错误' : 'Error',
    } satisfies Record<ConnectionState, string>,
    setupState: {
      pending: zh ? '等待扫码' : 'Waiting for scan',
      completed: zh ? '扫码完成' : 'Setup completed',
      denied: zh ? '扫码被拒绝' : 'Setup denied',
      expired: zh ? '扫码已超时' : 'Setup expired',
      cancelled: zh ? '扫码已取消' : 'Setup cancelled',
      error: zh ? '扫码失败' : 'Setup failed',
    } satisfies Record<ImSetupSession['status'], string>,
    platform: {
      feishu: zh ? '飞书' : 'Feishu',
      wecom: zh ? '企业微信' : 'WeCom',
      wecom_callback: zh ? '企业微信自建应用' : 'WeCom self-built app',
    } satisfies Record<ImPlatform, string>,
    dmOptions: [
      { value: 'pairing', label: zh ? '配对' : 'Pairing' },
      { value: 'allowlist', label: zh ? '允许列表' : 'Allowlist' },
      { value: 'open', label: zh ? '开放' : 'Open' },
      { value: 'disabled', label: zh ? '关闭' : 'Disabled' },
    ] satisfies { value: DmPolicy, label: string }[],
    groupOptions: [
      { value: 'allowlist', label: zh ? '允许列表' : 'Allowlist' },
      { value: 'open', label: zh ? '开放' : 'Open' },
      { value: 'disabled', label: zh ? '关闭' : 'Disabled' },
    ] satisfies { value: GroupPolicy, label: string }[],
    domainOptions: [
      { value: 'feishu', label: zh ? '飞书' : 'Feishu' },
      { value: 'lark', label: 'Lark' },
    ] satisfies { value: FeishuDomain, label: string }[],
    connectionOptions: [
      { value: 'websocket', label: 'WebSocket' },
      { value: 'webhook', label: 'Webhook' },
    ] satisfies { value: FeishuConnectionMode, label: string }[],
  }
}

function formatUtc(value: number, lang: Lang): string {
  const date = new Date(unixMs(value))
  if (Number.isNaN(date.getTime())) return ''
  return new Intl.DateTimeFormat(lang === 'zh' ? 'zh-CN' : 'en-US', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
    timeZone: 'UTC',
  }).format(date)
}

function IdListField({ id, label, description, value, onChange }: {
  id: string
  label: string
  description?: string
  value: string[]
  onChange: (value: string[]) => void
}) {
  const [text, setText] = useState(value.join('\n'))
  const focused = useRef(false)
  const serialized = value.join('\n')
  useEffect(() => {
    if (!focused.current) setText(serialized)
  }, [serialized])
  return (
    <FieldBlock label={label} description={description} htmlFor={id}>
      <TextArea
        id={id}
        rows={3}
        value={text}
        onFocus={() => { focused.current = true }}
        onBlur={() => { focused.current = false; setText(value.join('\n')) }}
        onChange={(next) => {
          setText(next)
          onChange(parseIdList(next))
        }}
      />
    </FieldBlock>
  )
}

function GroupUsersEditor({ idPrefix, label, addLabel, groupLabel, usersLabel, removeLabel, value, onChange }: {
  idPrefix: string
  label: string
  addLabel: string
  groupLabel: string
  usersLabel: string
  removeLabel: string
  value: Record<string, string[]>
  onChange: (value: Record<string, string[]>) => void
}) {
  const toRows = (map: Record<string, string[]>) => Object.entries(map).map(([groupId, users]) => ({ groupId, users: users.join('\n') }))
  const [rows, setRows] = useState(() => toRows(value))
  const emitted = useRef(JSON.stringify(value))
  useEffect(() => {
    const next = JSON.stringify(value)
    if (next !== emitted.current) {
      emitted.current = next
      setRows(toRows(value))
    }
  }, [value])

  const commit = (nextRows: { groupId: string, users: string }[]) => {
    setRows(nextRows)
    const map: Record<string, string[]> = {}
    for (const row of nextRows) {
      const groupId = row.groupId.trim()
      if (!groupId) continue
      map[groupId] = parseIdList(row.users)
    }
    emitted.current = JSON.stringify(map)
    onChange(map)
  }

  return (
    <FieldBlock label={label}>
      <div className="flex flex-col gap-2">
        {rows.map((row, index) => (
          <div key={`${idPrefix}-${index}`} className="flex flex-col gap-2">
            <Input
              aria-label={`${groupLabel} ${index + 1}`}
              value={row.groupId}
              onChange={(groupId) => commit(rows.map((item, itemIndex) => itemIndex === index ? { ...item, groupId } : item))}
            />
            <TextArea
              aria-label={`${usersLabel} ${index + 1}`}
              rows={2}
              value={row.users}
              onChange={(users) => commit(rows.map((item, itemIndex) => itemIndex === index ? { ...item, users } : item))}
            />
            <Button
              size="sm"
              variant="danger"
              onClick={() => commit(rows.filter((_, itemIndex) => itemIndex !== index))}
            >
              {removeLabel} {index + 1}
            </Button>
          </div>
        ))}
        <Button size="sm" onClick={() => commit([...rows, { groupId: '', users: '' }])}>{addLabel}</Button>
      </div>
    </FieldBlock>
  )
}

function AccessEditor({ idPrefix, copy, access, onChange }: {
  idPrefix: string
  copy: Copy
  access: ImAccessConfig
  onChange: (access: ImAccessConfig) => void
}) {
  return (
    <>
      <FieldBlock label={`${idPrefix} ${copy.dm}`}>
        <Select ariaLabel={`${idPrefix} ${copy.dm}`} value={access.dmPolicy} options={copy.dmOptions} onChange={(dmPolicy) => onChange({ ...access, dmPolicy: dmPolicy as DmPolicy })} />
      </FieldBlock>
      <FieldBlock label={`${idPrefix} ${copy.group}`}>
        <Select ariaLabel={`${idPrefix} ${copy.group}`} value={access.groupPolicy} options={copy.groupOptions} onChange={(groupPolicy) => onChange({ ...access, groupPolicy: groupPolicy as GroupPolicy })} />
      </FieldBlock>
      <IdListField id={`${idPrefix}-users`} label={`${idPrefix} ${copy.users}`} description={copy.usersHint} value={access.allowedUsers} onChange={(allowedUsers) => onChange({ ...access, allowedUsers })} />
      <IdListField id={`${idPrefix}-groups`} label={`${idPrefix} ${copy.groups}`} description={copy.groupsHint} value={access.allowedGroups} onChange={(allowedGroups) => onChange({ ...access, allowedGroups })} />
      <GroupUsersEditor
        idPrefix={idPrefix}
        label={`${idPrefix} ${copy.groupUsers}`}
        addLabel={copy.addGroupUsers}
        groupLabel={`${idPrefix} ${copy.groupId}`}
        usersLabel={`${idPrefix} ${copy.groupUserIds}`}
        removeLabel={copy.removeGroup}
        value={access.groupUsers}
        onChange={(groupUsers) => onChange({ ...access, groupUsers })}
      />
    </>
  )
}

function WebhookEditor({ idPrefix, copy, webhook, onChange }: {
  idPrefix: string
  copy: Copy
  webhook: ImWebhookConfig
  onChange: (webhook: ImWebhookConfig) => void
}) {
  const [portText, setPortText] = useState(String(webhook.port))
  const focused = useRef(false)
  useEffect(() => {
    if (!focused.current) setPortText(String(webhook.port))
  }, [webhook.port])
  return (
    <>
      <FieldBlock label={`${idPrefix} ${copy.host}`} htmlFor={`${idPrefix}-host`}>
        <Input id={`${idPrefix}-host`} value={webhook.host} onChange={(host) => onChange({ ...webhook, host })} />
      </FieldBlock>
      <FieldBlock label={`${idPrefix} ${copy.port}`} htmlFor={`${idPrefix}-port`}>
        <Input
          id={`${idPrefix}-port`}
          inputMode="numeric"
          value={portText}
          onFocus={() => { focused.current = true }}
          onBlur={() => { focused.current = false; setPortText(String(webhook.port)) }}
          onChange={(next) => {
            setPortText(next)
            if (!/^\d{1,5}$/.test(next)) return
            const port = Number(next)
            if (port >= 1 && port <= 65535) onChange({ ...webhook, port })
          }}
        />
      </FieldBlock>
      <FieldBlock label={`${idPrefix} ${copy.path}`} htmlFor={`${idPrefix}-path`}>
        <Input id={`${idPrefix}-path`} value={webhook.path} onChange={(path) => onChange({ ...webhook, path })} />
      </FieldBlock>
    </>
  )
}

function SecretInput({ label, value, onChange }: { label: string, value: string, onChange: (value: string) => void }) {
  return (
    <FieldBlock label={label} htmlFor={label}>
      <Input id={label} type="password" autoComplete="off" value={value} onChange={onChange} />
    </FieldBlock>
  )
}

export function ImTab({ lang, config, providers, onChange, onFlush }: {
  lang: Lang
  config: ImConfig | null
  providers: ModelProvider[]
  onChange: (config: ImConfig) => void
  onFlush: () => Promise<boolean>
}) {
  const copy = imCopy(lang)
  const configRef = useRef(config)
  configRef.current = config
  const onChangeRef = useRef(onChange)
  onChangeRef.current = onChange
  const onFlushRef = useRef(onFlush)
  onFlushRef.current = onFlush
  const mountedRef = useRef(true)
  const setupGen = useRef(0)
  const statusGen = useRef(0)
  const pairingGen = useRef(0)
  const credentialGen = useRef(0)
  const sessionRef = useRef<string | null>(null)
  const pollAbortRef = useRef<AbortController | null>(null)
  const secretsRef = useRef<Record<ImPlatform, SecretDraft>>({
    feishu: emptySecrets(),
    wecom: emptySecrets(),
    wecom_callback: emptySecrets(),
  })

  const [status, setStatus] = useState<ImStatus[] | null>(null)
  const [statusError, setStatusError] = useState('')
  const [requests, setRequests] = useState<ImPairingRequest[] | null>(null)
  const [approved, setApproved] = useState<ImApprovedUser[] | null>(null)
  const [pairingError, setPairingError] = useState('')
  const [secrets, setSecrets] = useState<Record<ImPlatform, SecretDraft>>(secretsRef.current)
  const [credentialNote, setCredentialNote] = useState<Record<ImPlatform, string>>({
    feishu: '', wecom: '', wecom_callback: '',
  })
  const [credentialError, setCredentialError] = useState<Record<ImPlatform, string>>({
    feishu: '', wecom: '', wecom_callback: '',
  })
  const [busyCredential, setBusyCredential] = useState<ImPlatform | null>(null)
  const [setupSession, setSetupSession] = useState<ImSetupSession | null>(null)
  const [setupError, setSetupError] = useState('')
  const [setupBusy, setSetupBusy] = useState(false)
  const [qrUrl, setQrUrl] = useState('')
  const [qrError, setQrError] = useState('')
  const [copied, setCopied] = useState('')
  const [dirError, setDirError] = useState('')
  const [actionError, setActionError] = useState('')
  const [busyAction, setBusyAction] = useState('')

  useEffect(() => {
    secretsRef.current = secrets
  }, [secrets])

  const patchNote = (platform: ImPlatform, note: string, error: string) => {
    setCredentialNote((current) => ({ ...current, [platform]: note }))
    setCredentialError((current) => ({ ...current, [platform]: error }))
  }

  const refreshStatus = useCallback(async () => {
    const gen = ++statusGen.current
    try {
      const rows = await getImStatus()
      if (!mountedRef.current || gen !== statusGen.current) return
      setStatus(rows)
      setStatusError('')
    } catch (error) {
      if (!mountedRef.current || gen !== statusGen.current) return
      setStatusError(errorText(error, copy.requestFailed))
    }
  }, [copy.requestFailed])

  const refreshPairing = useCallback(async () => {
    const gen = ++pairingGen.current
    try {
      const [nextRequests, nextApproved] = await Promise.all([
        listImPairingRequests(),
        listImApprovedUsers(),
      ])
      if (!mountedRef.current || gen !== pairingGen.current) return 'stale' as const
      setRequests(nextRequests)
      setApproved(nextApproved)
      setPairingError('')
      return 'applied' as const
    } catch (error) {
      if (!mountedRef.current || gen !== pairingGen.current) return 'stale' as const
      setPairingError(errorText(error, copy.requestFailed))
      return 'error' as const
    }
  }, [copy.requestFailed])

  const hasConfig = config != null
  useEffect(() => {
    mountedRef.current = true
    if (!hasConfig) {
      return () => {
        mountedRef.current = false
      }
    }
    void refreshStatus()
    void refreshPairing()
    let unlistenStatus: (() => void) | undefined
    let unlistenPairing: (() => void) | undefined
    let alive = true
    void subscribeImStatus(() => { if (alive) void refreshStatus() }).then((unlisten) => {
      if (alive) unlistenStatus = unlisten
      else unlisten()
    })
    void subscribeImPairing(() => { if (alive) void refreshPairing() }).then((unlisten) => {
      if (alive) unlistenPairing = unlisten
      else unlisten()
    })
    return () => {
      alive = false
      mountedRef.current = false
      setupGen.current += 1
      pollAbortRef.current?.abort()
      const id = sessionRef.current
      sessionRef.current = null
      if (id) void cancelImSetup(id).catch(() => {})
      unlistenStatus?.()
      unlistenPairing?.()
    }
  }, [hasConfig, refreshPairing, refreshStatus])

  const update = (next: ImConfig) => {
    configRef.current = next
    onChangeRef.current(next)
  }

  const watchSetup = async (id: string, gen: number, signal: AbortSignal) => {
    try {
      while (gen === setupGen.current && !signal.aborted) {
        await sleep(IM_SETUP_POLL_MS, signal)
        if (gen !== setupGen.current || signal.aborted || !mountedRef.current) return
        const next = await pollImSetup(id)
        if (gen !== setupGen.current || signal.aborted || !mountedRef.current) return
        if (next.status === 'pending' && pastExpiry(next)) {
          sessionRef.current = null
          setSetupSession({ ...next, status: 'expired', message: copy.timeout })
          void cancelImSetup(id).catch(() => {})
          return
        }
        setSetupSession(next)
        if (next.status === 'completed') {
          sessionRef.current = null
          const current = configRef.current
          if (next.identity && current) update(applyImSetupIdentity(current, next))
          else setSetupError(copy.identityMissing)
          return
        }
        if (TERMINAL_SETUP[next.status]) {
          sessionRef.current = null
          return
        }
      }
    } catch (error) {
      if (isAbort(error) || gen !== setupGen.current || !mountedRef.current) return
      setSetupError(errorText(error, copy.requestFailed))
    }
  }

  const startSetup = async (platform: ImPlatform) => {
    const current = configRef.current
    if (!current) return
    setupGen.current += 1
    pollAbortRef.current?.abort()
    const previous = sessionRef.current
    sessionRef.current = null
    if (previous) void cancelImSetup(previous).catch(() => {})
    const gen = setupGen.current
    setSetupError('')
    setQrError('')
    setQrUrl('')
    setSetupBusy(true)
    try {
      const domain = platform === 'feishu' ? current.feishu.domain : null
      const session = await beginImSetup(platform, domain)
      if (gen !== setupGen.current || !mountedRef.current) {
        void cancelImSetup(session.id).catch(() => {})
        return
      }
      if (!isHttpUrl(session.url)) {
        sessionRef.current = session.id
        setSetupSession(session)
        setSetupError(session.url ? copy.badUrl : copy.missingUrl)
        return
      }
      if (session.status === 'pending' && pastExpiry(session)) {
        setSetupSession({ ...session, status: 'expired', message: copy.timeout })
        void cancelImSetup(session.id).catch(() => {})
        return
      }
      sessionRef.current = session.id
      setSetupSession(session)
      if (session.status === 'completed') {
        sessionRef.current = null
        if (session.identity) update(applyImSetupIdentity(configRef.current ?? current, session))
        else setSetupError(copy.identityMissing)
        return
      }
      if (TERMINAL_SETUP[session.status]) {
        sessionRef.current = null
        return
      }
      const abort = new AbortController()
      pollAbortRef.current = abort
      void watchSetup(session.id, gen, abort.signal)
    } catch (error) {
      if (gen === setupGen.current && mountedRef.current) setSetupError(errorText(error, copy.requestFailed))
    } finally {
      if (gen === setupGen.current && mountedRef.current) setSetupBusy(false)
    }
  }

  const cancelSetup = async () => {
    setupGen.current += 1
    pollAbortRef.current?.abort()
    const id = sessionRef.current
    sessionRef.current = null
    if (id) {
      try {
        await cancelImSetup(id)
        if (mountedRef.current) {
          setSetupSession((current) => current && current.id === id ? { ...current, status: 'cancelled', message: copy.cancelled } : current)
          setSetupError('')
        }
      } catch (error) {
        if (mountedRef.current) setSetupError(errorText(error, copy.requestFailed))
      }
    } else if (mountedRef.current) {
      setSetupSession((current) => current ? { ...current, status: 'cancelled', message: copy.cancelled } : current)
    }
  }

  useEffect(() => {
    const url = setupSession?.url ?? ''
    if (!url || !isHttpUrl(url)) {
      setQrUrl('')
      return
    }
    let alive = true
    const gen = setupGen.current
    setQrError('')
    void authorizationQrDataUrl(url).then((dataUrl) => {
      if (!alive || gen !== setupGen.current || !mountedRef.current) return
      if (!dataUrl.startsWith('data:image/')) {
        setQrError(copy.requestFailed)
        setQrUrl('')
        return
      }
      setQrUrl(dataUrl)
    }).catch((error) => {
      if (!alive || gen !== setupGen.current || !mountedRef.current) return
      setQrUrl('')
      setQrError(errorText(error, copy.requestFailed))
    })
    return () => { alive = false }
  }, [copy.requestFailed, setupSession?.url])

  const saveCredentials = async (platform: ImPlatform) => {
    const submitted = { ...secretsRef.current[platform] }
    if (!submitted.secret.trim()) {
      patchNote(platform, '', copy.requestFailed)
      return
    }
    const gen = ++credentialGen.current
    setBusyCredential(platform)
    patchNote(platform, '', '')
    try {
      const flushed = await onFlushRef.current()
      if (!mountedRef.current || gen !== credentialGen.current) return
      if (!flushed) {
        patchNote(platform, '', copy.flushFailed)
        return
      }
      const current = configRef.current
      const identity = !current ? ''
        : platform === 'feishu' ? current.feishu.appId
          : platform === 'wecom' ? current.wecom.botId
            : current.wecomCallback.corpId
      if (!identity.trim()) {
        patchNote(platform, '', copy.identityRequired)
        return
      }
      await saveImCredentials(platform, submitted)
      if (!mountedRef.current || gen !== credentialGen.current) return
      const unchanged = sameSecrets(secretsRef.current[platform], submitted)
      setSecrets((current) => sameSecrets(current[platform], submitted)
        ? { ...current, [platform]: emptySecrets() }
        : current)
      patchNote(platform, unchanged ? copy.saveOk : copy.saveKept, '')
      await refreshStatus()
    } catch (error) {
      if (!mountedRef.current || gen !== credentialGen.current) return
      patchNote(platform, '', errorText(error, copy.requestFailed))
    } finally {
      if (mountedRef.current && gen === credentialGen.current) setBusyCredential((current) => current === platform ? null : current)
    }
  }

  const clearCredentials = async (platform: ImPlatform) => {
    const accepted = await confirmDialog({ message: copy.clearConfirm, confirmLabel: copy.clear, danger: true })
    if (!accepted || !mountedRef.current) return
    const gen = ++credentialGen.current
    setBusyCredential(platform)
    patchNote(platform, '', '')
    try {
      await clearImCredentials(platform)
      if (!mountedRef.current || gen !== credentialGen.current) return
      setSecrets((current) => ({ ...current, [platform]: emptySecrets() }))
      patchNote(platform, copy.clearOk, '')
      await refreshStatus()
    } catch (error) {
      if (!mountedRef.current || gen !== credentialGen.current) return
      patchNote(platform, '', errorText(error, copy.requestFailed))
    } finally {
      if (mountedRef.current && gen === credentialGen.current) setBusyCredential((current) => current === platform ? null : current)
    }
  }

  const runPairingAction = async (key: string, action: () => Promise<void>) => {
    setBusyAction(key)
    setActionError('')
    try {
      await action()
    } catch (error) {
      if (mountedRef.current) setActionError(errorText(error, copy.requestFailed))
      return
    } finally {
      if (mountedRef.current) setBusyAction('')
    }
    const refreshed = await refreshPairing()
    if (refreshed === 'error' && mountedRef.current) setActionError(copy.refreshFailed)
  }

  const reconnect = async (platform: ImPlatform) => {
    setActionError('')
    try {
      await reconnectIm(platform)
      await refreshStatus()
    } catch (error) {
      if (mountedRef.current) setActionError(errorText(error, copy.requestFailed))
    }
  }

  const copyWebhook = async (url: string) => {
    try {
      await navigator.clipboard.writeText(url)
      setCopied(url)
    } catch (error) {
      setActionError(errorText(error, copy.requestFailed))
    }
  }

  if (!config) {
    return (
      <section aria-label={copy.agentTitle}>
        <p role="alert">{copy.missing}</p>
      </section>
    )
  }

  const statusOf = (platform: ImPlatform) => status?.find((row) => row.platform === platform) ?? null

  const renderStatus = (platform: ImPlatform, enabled: boolean) => {
    const row = statusOf(platform)
    return (
      <div className="flex flex-col gap-2 py-2">
        {row ? (
          <>
            <p role="status">{copy.state[row.state]}{row.message ? ` · ${row.message}` : ''}</p>
            <p>{row.credentialsConfigured ? copy.configured : copy.notConfigured}</p>
            {enabled && !row.credentialsConfigured ? <p role="alert">{copy.enabledMissing}</p> : null}
            {row.webhookUrl ? (
              <div className="flex flex-col gap-2">
                {isHttpUrl(row.webhookUrl) ? <a href={row.webhookUrl}>{row.webhookUrl}</a> : <p>{row.webhookUrl}</p>}
                <Button size="sm" onClick={() => void copyWebhook(row.webhookUrl)}>{copy.copyUrl}</Button>
                {copied === row.webhookUrl ? <p role="status">{copy.copied}</p> : null}
              </div>
            ) : null}
            {row.lastMessageAt != null ? <p>{copy.lastMessage}: {formatUtc(row.lastMessageAt, lang)}</p> : null}
          </>
        ) : <p>{copy.noStatus}</p>}
        <Button size="sm" onClick={() => void reconnect(platform)}>{copy.reconnect} {copy.platform[platform]}</Button>
      </div>
    )
  }

  const renderSecrets = (platform: ImPlatform, fields: { key: keyof SecretDraft, label: string }[]) => {
    const draft = secrets[platform]
    return (
      <div className="flex flex-col gap-2">
        <p className="kv-row-desc">{copy.credentialHint}</p>
        {fields.map((field) => (
          <SecretInput
            key={field.key}
            label={field.label}
            value={draft[field.key]}
            onChange={(value) => setSecrets((current) => ({ ...current, [platform]: { ...current[platform], [field.key]: value } }))}
          />
        ))}
        <div className="flex gap-2">
          <Button size="sm" variant="primary" aria-label={`${copy.save} ${copy.platform[platform]}`} disabled={busyCredential === platform} onClick={() => void saveCredentials(platform)}>{copy.save}</Button>
          <Button size="sm" variant="danger" aria-label={`${copy.clear} ${copy.platform[platform]}`} disabled={busyCredential === platform} onClick={() => void clearCredentials(platform)}>{copy.clear}</Button>
        </div>
        {credentialNote[platform] ? <p role="status">{credentialNote[platform]}</p> : null}
        {credentialError[platform] ? <p role="alert">{credentialError[platform]}</p> : null}
      </div>
    )
  }

  const chooseDirectory = async () => {
    setDirError('')
    try {
      const selected = await open({ directory: true, multiple: false })
      if (typeof selected !== 'string' || !configRef.current) return
      update({ ...configRef.current, agent: { ...configRef.current.agent, workingDirectory: selected } })
    } catch (error) {
      setDirError(errorText(error, copy.dirFailed))
    }
  }

  const patchAgent = (patch: Partial<ImAgentConfig>) => {
    if (!configRef.current) return
    update({ ...configRef.current, agent: { ...configRef.current.agent, ...patch } })
  }
  const patchFeishu = (patch: Partial<FeishuConfig>) => {
    if (!configRef.current) return
    update({ ...configRef.current, feishu: { ...configRef.current.feishu, ...patch } })
  }
  const patchWecom = (patch: Partial<WecomConfig>) => {
    if (!configRef.current) return
    update({ ...configRef.current, wecom: { ...configRef.current.wecom, ...patch } })
  }
  const patchCallback = (patch: Partial<WecomCallbackConfig>) => {
    if (!configRef.current) return
    update({ ...configRef.current, wecomCallback: { ...configRef.current.wecomCallback, ...patch } })
  }

  return (
    <div className="flex flex-col gap-4">
      <section aria-label={copy.agentTitle}>
        {copy.pageWarnings.map((warning) => <p key={warning} className="kv-row-desc">{warning}</p>)}
      </section>

      {setupError ? <p role="alert">{setupError}</p> : null}
      {setupSession ? (
        <section aria-label={copy.setup} className="kv-group">
          <div className="kv-group-title">{copy.setup} · {copy.platform[setupSession.platform]}</div>
          <p role="status">{copy.setupState[setupSession.status]}{setupSession.message ? ` · ${setupSession.message}` : ''}</p>
          {isHttpUrl(setupSession.url) ? <a href={setupSession.url} target="_blank" rel="noreferrer">{setupSession.url}</a> : null}
          {qrUrl ? <img alt={copy.qrAlt} src={qrUrl} width={220} height={220} /> : null}
          {qrError ? <p role="alert">{qrError}</p> : null}
          {setupSession.identity ? (
            <p>{copy.botName}: {setupSession.identity.botName || '—'} · {copy.owner}: {setupSession.identity.ownerId || '—'}</p>
          ) : null}
          {setupSession.status === 'pending' ? (
            <Button size="sm" onClick={() => void cancelSetup()}>{copy.cancelSetup}</Button>
          ) : null}
        </section>
      ) : null}

      <SettingsGroup title={copy.agentTitle}>
        <p className="kv-row-desc">{copy.agentHint}</p>
        <FieldBlock label={copy.assistant} htmlFor="im-assistant-id">
          <Input id="im-assistant-id" value={config.agent.assistantId} onChange={(assistantId) => patchAgent({ assistantId })} />
        </FieldBlock>
        <FieldBlock label={copy.model}>
          <ModelPairSelect
            ariaLabel={copy.model}
            providerId={config.agent.providerId}
            model={config.agent.model}
            providers={providers}
            inheritLabel={lang === 'zh' ? '跟随桌面对话默认模型' : 'Use the desktop default model'}
            onChange={(providerId, model) => patchAgent({ providerId, model })}
          />
        </FieldBlock>
        <FieldBlock label={copy.workdir} description={copy.workdirHint} htmlFor="im-working-directory">
          <div className="flex gap-2">
            <Input id="im-working-directory" className="min-w-0 flex-1" value={config.agent.workingDirectory} onChange={(workingDirectory) => patchAgent({ workingDirectory })} />
            <Button size="sm" onClick={() => void chooseDirectory()}>{copy.chooseDir}</Button>
            <Button size="sm" onClick={() => patchAgent({ workingDirectory: '' })}>{copy.clearDir}</Button>
          </div>
        </FieldBlock>
        {dirError ? <p role="alert">{dirError}</p> : null}
        <SettingRow label={copy.groupSessions}>
          <Toggle ariaLabel={copy.groupSessions} checked={config.agent.groupSessionsPerUser} onChange={(groupSessionsPerUser) => patchAgent({ groupSessionsPerUser })} />
        </SettingRow>
        <SettingRow label={copy.streaming}>
          <Toggle ariaLabel={copy.streaming} checked={config.agent.streaming} onChange={(streaming) => patchAgent({ streaming })} />
        </SettingRow>
        <FieldBlock label={copy.homeFeishu} htmlFor="im-home-feishu">
          <Input id="im-home-feishu" value={config.feishu.homeChannel} onChange={(homeChannel) => patchFeishu({ homeChannel })} />
        </FieldBlock>
        <FieldBlock label={copy.homeWecom} htmlFor="im-home-wecom">
          <Input id="im-home-wecom" value={config.wecom.homeChannel} onChange={(homeChannel) => patchWecom({ homeChannel })} />
        </FieldBlock>
        <FieldBlock label={copy.homeCallback} htmlFor="im-home-callback">
          <Input id="im-home-callback" value={config.wecomCallback.homeChannel} onChange={(homeChannel) => patchCallback({ homeChannel })} />
        </FieldBlock>
      </SettingsGroup>

      <SettingsGroup title={copy.feishu}>
        <section aria-label={copy.feishu}>
          <SettingRow label={copy.enabledFeishu}>
            <Toggle ariaLabel={copy.enabledFeishu} checked={config.feishu.enabled} onChange={(enabled) => patchFeishu({ enabled })} />
          </SettingRow>
          {renderStatus('feishu', config.feishu.enabled)}
          <FieldBlock label={copy.appId} htmlFor="im-feishu-app-id">
            <Input id="im-feishu-app-id" value={config.feishu.appId} onChange={(appId) => patchFeishu({ appId })} />
          </FieldBlock>
          <FieldBlock label={copy.domain}>
            <Select ariaLabel={copy.domain} value={config.feishu.domain} options={copy.domainOptions} onChange={(domain) => patchFeishu({ domain: domain as FeishuDomain })} />
          </FieldBlock>
          <FieldBlock label={copy.connection}>
            <Select ariaLabel={copy.connection} value={config.feishu.connectionMode} options={copy.connectionOptions} onChange={(connectionMode) => patchFeishu({ connectionMode: connectionMode as FeishuConnectionMode })} />
          </FieldBlock>
          {config.feishu.connectionMode === 'webhook' ? (
            <WebhookEditor idPrefix={copy.feishu} copy={copy} webhook={config.feishu.webhook} onChange={(webhook) => patchFeishu({ webhook })} />
          ) : null}
          <AccessEditor idPrefix={copy.feishu} copy={copy} access={config.feishu.access} onChange={(access) => patchFeishu({ access })} />
          <SettingRow label={copy.mention}>
            <Toggle ariaLabel={copy.mention} checked={config.feishu.requireMention} onChange={(requireMention) => patchFeishu({ requireMention })} />
          </SettingRow>
          {renderSecrets('feishu', [
            { key: 'secret', label: copy.secretFeishu },
            ...(config.feishu.connectionMode === 'webhook' ? [
              { key: 'encryptKey' as const, label: copy.encryptKey },
              { key: 'verificationToken' as const, label: copy.verification },
            ] : []),
          ])}
          <Button size="sm" disabled={setupBusy} onClick={() => void startSetup('feishu')}>{copy.scanFeishu}</Button>
        </section>
      </SettingsGroup>

      <SettingsGroup title={copy.wecom}>
        <section aria-label={copy.wecom}>
          <SettingRow label={copy.enabledWecom}>
            <Toggle ariaLabel={copy.enabledWecom} checked={config.wecom.enabled} onChange={(enabled) => patchWecom({ enabled })} />
          </SettingRow>
          {renderStatus('wecom', config.wecom.enabled)}
          <FieldBlock label={copy.botId} htmlFor="im-wecom-bot-id">
            <Input id="im-wecom-bot-id" value={config.wecom.botId} onChange={(botId) => patchWecom({ botId })} />
          </FieldBlock>
          <FieldBlock label={copy.websocketUrl} htmlFor="im-wecom-ws">
            <Input id="im-wecom-ws" value={config.wecom.websocketUrl} onChange={(websocketUrl) => patchWecom({ websocketUrl })} />
          </FieldBlock>
          <AccessEditor idPrefix={copy.wecom} copy={copy} access={config.wecom.access} onChange={(access) => patchWecom({ access })} />
          {renderSecrets('wecom', [{ key: 'secret', label: copy.secretWecom }])}
          <Button size="sm" disabled={setupBusy} onClick={() => void startSetup('wecom')}>{copy.scanWecom}</Button>
        </section>
      </SettingsGroup>

      <SettingsGroup title={copy.callback}>
        <section aria-label={copy.callback}>
          <SettingRow label={copy.enabledCallback}>
            <Toggle ariaLabel={copy.enabledCallback} checked={config.wecomCallback.enabled} onChange={(enabled) => patchCallback({ enabled })} />
          </SettingRow>
          {renderStatus('wecom_callback', config.wecomCallback.enabled)}
          <FieldBlock label={copy.corpId} htmlFor="im-corp-id">
            <Input id="im-corp-id" value={config.wecomCallback.corpId} onChange={(corpId) => patchCallback({ corpId })} />
          </FieldBlock>
          <FieldBlock label={copy.agentId} htmlFor="im-agent-id">
            <Input id="im-agent-id" value={config.wecomCallback.agentId} onChange={(agentId) => patchCallback({ agentId })} />
          </FieldBlock>
          <WebhookEditor idPrefix={copy.callback} copy={copy} webhook={config.wecomCallback.webhook} onChange={(webhook) => patchCallback({ webhook })} />
          <AccessEditor idPrefix={copy.callback} copy={copy} access={config.wecomCallback.access} onChange={(access) => patchCallback({ access })} />
          {renderSecrets('wecom_callback', [
            { key: 'secret', label: copy.secretCallback },
            { key: 'token', label: copy.callbackToken },
            { key: 'encodingAesKey', label: copy.aes },
          ])}
        </section>
      </SettingsGroup>

      <SettingsGroup title={copy.pairing}>
        <section aria-label={copy.pairing}>
          <p className="kv-row-desc">{copy.pairingHint}</p>
          {statusError ? <p role="alert">{copy.statusFailed}: {statusError}</p> : null}
          {statusError ? <Button size="sm" onClick={() => void refreshStatus()}>{copy.retryStatus}</Button> : null}
          {pairingError ? <p role="alert">{copy.pairingFailed}: {pairingError}</p> : null}
          {pairingError ? <Button size="sm" onClick={() => void refreshPairing()}>{copy.retryPairing}</Button> : null}
          {actionError ? <p role="alert">{actionError}</p> : null}
          <div className="kv-group-title">{copy.pendingTitle}</div>
          {requests && requests.length === 0 ? <p>{copy.noPending}</p> : null}
          {requests?.map((request) => {
            const key = `${request.platform}:${request.code}`
            return (
              <div key={key} className="flex flex-col gap-2 py-2">
                <p>{copy.platform[request.platform]} · {request.userName || request.userId} · {request.userId} · {request.code}</p>
                <div className="flex gap-2">
                  <Button size="sm" disabled={busyAction === key} onClick={() => void runPairingAction(key, () => approveImPairing(request.platform, request.code))}>{copy.approve}</Button>
                  <Button size="sm" disabled={busyAction === key} onClick={() => void runPairingAction(key, () => denyImPairing(request.platform, request.code))}>{copy.deny}</Button>
                </div>
              </div>
            )
          })}
          <div className="kv-group-title">{copy.approvedTitle}</div>
          {approved && approved.length === 0 ? <p>{copy.noApproved}</p> : null}
          {approved?.map((user) => {
            const key = `revoke:${user.platform}:${user.identity}:${user.userId}`
            return (
              <div key={key} className="flex flex-col gap-2 py-2">
                <p>{copy.platform[user.platform]} · {user.userName || user.userId} · {user.userId} · {copy.boundId}: {user.identity || '—'}{user.approvedAt ? ` · ${formatUtc(user.approvedAt, lang)}` : ''}</p>
                <Button size="sm" variant="danger" disabled={busyAction === key} onClick={() => void runPairingAction(key, () => revokeImUser(user.platform, user.userId))}>{copy.revoke}</Button>
              </div>
            )
          })}
        </section>
      </SettingsGroup>
    </div>
  )
}
