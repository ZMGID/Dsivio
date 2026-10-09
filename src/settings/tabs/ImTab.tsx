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

const TERMINAL_SETUP: Record<string, true> = {
  completed: true, denied: true, expired: true, cancelled: true, error: true,
}

type SecretDraft = CredentialInput
type Copy = ReturnType<typeof imCopy>
type PlatformGen = Record<ImPlatform, number>

function emptySecrets(): SecretDraft {
  return { secret: '', encryptKey: '', verificationToken: '', token: '', encodingAesKey: '' }
}

function emptyPlatformGen(): PlatformGen {
  return { feishu: 0, wecom: 0, wecom_callback: 0 }
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

function isSecureSetupUrl(url: string): boolean {
  try {
    return new URL(url).protocol === 'https:'
  } catch {
    return false
  }
}

function pastExpiry(session: ImSetupSession, now = Date.now()): boolean {
  return unixMs(session.expiresAt) <= now
}

function setupStatus(session: ImSetupSession): ImSetupSession['status'] {
  return session.status
}

function platformIdentity(config: ImConfig, platform: ImPlatform): string {
  if (platform === 'feishu') return config.feishu.appId.trim()
  if (platform === 'wecom') return config.wecom.botId.trim()
  return config.wecomCallback.corpId.trim()
}

function stagedIdentity(session: ImSetupSession): string {
  const identity = session.identity
  if (!identity) return ''
  if (session.platform === 'feishu') return identity.appId.trim()
  if (session.platform === 'wecom') return identity.botId.trim()
  return ''
}

function withEnabled(config: ImConfig, platform: ImPlatform, enabled: boolean): ImConfig {
  if (platform === 'feishu') return { ...config, feishu: { ...config.feishu, enabled } }
  if (platform === 'wecom') return { ...config, wecom: { ...config.wecom, enabled } }
  return { ...config, wecomCallback: { ...config.wecomCallback, enabled } }
}

/** QR identity is applied before commit. Feishu stays disabled and on websocket; webhook fields stay. */
function applyAuthorizedIdentity(config: ImConfig, session: ImSetupSession): ImConfig | null {
  const expected = stagedIdentity(session)
  if (!expected || !session.identity) return null
  if (session.platform === 'feishu') {
    return {
      ...config,
      feishu: {
        ...config.feishu,
        appId: expected,
        domain: session.identity.domain || config.feishu.domain,
        connectionMode: 'websocket',
        enabled: false,
      },
    }
  }
  if (session.platform === 'wecom') {
    return {
      ...config,
      wecom: { ...config.wecom, botId: expected, enabled: false },
    }
  }
  return null
}

function stillUncommittedDraft(config: ImConfig, platform: ImPlatform, expected: string): boolean {
  if (platformIdentity(config, platform) !== expected) return false
  if (platform === 'feishu') return !config.feishu.enabled && config.feishu.connectionMode === 'websocket'
  if (platform === 'wecom') return !config.wecom.enabled
  return false
}

type FeishuIdentityRestore = Pick<FeishuConfig, 'appId' | 'domain' | 'connectionMode' | 'enabled'>
type WecomIdentityRestore = Pick<WecomConfig, 'botId' | 'enabled'>

/** Fields this scan wrote before credentials were stored. Other platforms and shared fields stay out of it. */
type FlowRestore = {
  id: string
  platform: 'feishu' | 'wecom'
  expected: string
  reverted: boolean
  feishu?: FeishuIdentityRestore
  wecom?: WecomIdentityRestore
}

function captureFlowRestore(config: ImConfig, session: ImSetupSession, expected: string): FlowRestore | null {
  if (session.platform === 'feishu') {
    return {
      id: session.id,
      platform: 'feishu',
      expected,
      reverted: false,
      feishu: {
        appId: config.feishu.appId,
        domain: config.feishu.domain,
        connectionMode: config.feishu.connectionMode,
        enabled: config.feishu.enabled,
      },
    }
  }
  if (session.platform === 'wecom') {
    return {
      id: session.id,
      platform: 'wecom',
      expected,
      reverted: false,
      wecom: { botId: config.wecom.botId, enabled: config.wecom.enabled },
    }
  }
  return null
}

function configWithRestore(config: ImConfig, pending: FlowRestore): ImConfig {
  if (pending.platform === 'feishu' && pending.feishu) {
    return { ...config, feishu: { ...config.feishu, ...pending.feishu } }
  }
  if (pending.platform === 'wecom' && pending.wecom) {
    return { ...config, wecom: { ...config.wecom, ...pending.wecom } }
  }
  return config
}

function restoreApplied(config: ImConfig, pending: FlowRestore): boolean {
  if (pending.platform === 'feishu' && pending.feishu) {
    const row = config.feishu
    const previous = pending.feishu
    return row.appId === previous.appId
      && row.domain === previous.domain
      && row.connectionMode === previous.connectionMode
      && row.enabled === previous.enabled
  }
  if (pending.platform === 'wecom' && pending.wecom) {
    return config.wecom.botId === pending.wecom.botId && config.wecom.enabled === pending.wecom.enabled
  }
  return false
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
    advanced: zh ? '高级设置' : 'Advanced settings',
    agentTitle: zh ? '共享助手' : 'Shared assistant',
    agentHint: zh ? '模型留空则使用桌面对话的默认模型。工作目录和通知频道按这里保存。' : 'An empty model uses the desktop chat default. The working directory and home channels are saved here.',
    assistant: zh ? '助手 ID' : 'Assistant ID',
    model: zh ? '模型' : 'Model',
    inheritModel: zh ? '跟随桌面对话默认模型' : 'Use the desktop default model',
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
    enableNotSaved: zh ? '授权已保存，但启用没有写入。可以重试，当前还不是已连接。' : 'The authorization was saved, but enabling it was not. Retry is available; this is not connected yet.',
    identityRequired: zh ? '请先填写机器人 ID。凭证按这个 ID 保存。' : 'Enter the bot ID first. Credentials are stored for that ID.',
    identityChanged: zh ? '机器人 ID 已变化，凭证没有写入。' : 'The bot ID changed, so the credentials were not stored.',
    boundId: zh ? '绑定 ID' : 'Bound ID',
    configured: zh ? '凭证已配置' : 'Credentials configured',
    notConfigured: zh ? '凭证未配置' : 'Credentials not configured',
    enabledMissing: zh ? '已启用但未配置凭证，连接会失败。' : 'Enabled without credentials. The connection will fail.',
    saveOk: zh ? '凭证已保存' : 'Credentials saved',
    saveKept: zh ? '刚才提交的凭证已保存。输入框里后来改过的内容还没保存。' : 'The credentials you submitted were saved. Later edits in the fields are not saved yet.',
    clearOk: zh ? '凭证已清除' : 'Credentials cleared',
    requestFailed: zh ? '请求失败' : 'Request failed',
    scanConnect: zh ? '扫码连接' : 'Connect with QR',
    scanHint: {
      feishu: zh ? '打开飞书或 Lark，扫描二维码并在客户端里确认授权。' : 'Open Feishu or Lark, scan this QR code, and confirm the authorization in the app.',
      wecom: zh ? '打开企业微信，扫描二维码并在客户端里确认授权。' : 'Open WeCom, scan this QR code, and confirm the authorization in the app.',
    } satisfies Record<'feishu' | 'wecom', string>,
    setup: zh ? '扫码授权' : 'QR authorization',
    qrAlt: zh ? '授权二维码' : 'Authorization QR code',
    cancelSetup: zh ? '取消扫码' : 'Cancel setup',
    retrySave: zh ? '重试保存授权' : 'Retry saving authorization',
    savingAuth: zh ? '正在保存授权，连接状态以服务端为准。' : 'Saving the authorization. The connection state comes from the server.',
    missingUrl: zh ? '后端没有返回授权链接。' : 'The backend did not return an authorization link.',
    badUrl: zh ? '授权链接不是 https，已停止展示。' : 'The authorization link is not https, so it is not shown.',
    identityMissing: zh ? '扫码结果没有身份，设置没有改。' : 'The scan finished without an identity, so settings were left unchanged.',
    scanNotSubmitted: zh ? '这次扫码没有提交。' : 'This scan was not submitted.',
    timeout: zh ? '扫码已超时。' : 'QR setup timed out.',
    cancelled: zh ? '扫码已取消。' : 'QR setup was cancelled.',
    owner: zh ? '所有者' : 'Owner',
    botName: zh ? '机器人名称' : 'Bot name',
    reconnect: zh ? '重新连接' : 'Reconnect',
    disable: zh ? '停用' : 'Disable',
    copyUrl: zh ? '复制回调地址' : 'Copy callback URL',
    copied: zh ? '回调地址已复制' : 'Callback URL copied',
    lastMessage: zh ? '最近一条消息 (UTC)' : 'Last message (UTC)',
    noStatus: zh ? '尚未收到该平台状态。' : 'No status has arrived for this platform.',
    statusFailed: zh ? '状态刷新失败' : 'Status refresh failed',
    retryStatus: zh ? '重试刷新状态' : 'Retry status refresh',
    pairing: zh ? '配对' : 'Pairing',
    pairingHint: zh ? '待处理请求来自配对存储。批准或拒绝后列表以服务端结果为准。已批准用户可撤销。' : 'Pending requests come from the pairing store. After approve or deny, the list shows the server result. Approved users can be revoked.',
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
      authorized: zh ? '正在保存授权' : 'Saving authorization',
      completed: zh ? '授权已保存' : 'Authorization saved',
      denied: zh ? '扫码被拒绝' : 'Setup denied',
      expired: zh ? '扫码已超时' : 'Setup expired',
      cancelled: zh ? '扫码已取消' : 'Setup cancelled',
      error: zh ? '扫码失败' : 'Setup failed',
    } satisfies Record<string, string>,
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

function setupLabel(copy: Copy, status: ImSetupSession['status']): string {
  return copy.setupState[status]
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
        addLabel={`${idPrefix} ${copy.addGroupUsers}`}
        groupLabel={`${idPrefix} ${copy.groupId}`}
        usersLabel={`${idPrefix} ${copy.groupUserIds}`}
        removeLabel={`${idPrefix} ${copy.removeGroup}`}
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

function PlatformMark({ platform }: { platform: 'feishu' | 'wecom' }) {
  const title = platform === 'feishu' ? 'Feishu' : 'WeCom'
  return (
    <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true" focusable="false">
      <title>{title}</title>
      {platform === 'feishu' ? (
        <>
          <path d="M6.13732 3.80654C8.76716 5.98777 11.0232 8.49408 12.7428 11.4327L14.4397 9.75535C15.2458 8.96545 16.2113 8.34129 17.258 7.93496C16.7802 6.31936 16.0033 4.98005 14.9403 3.65376C14.8135 3.49449 14.6185 3.40346 14.4137 3.40346L6.28362 3.40021C6.06906 3.40021 5.9748 3.67001 6.13732 3.80654Z" fill="currentColor" />
          <path d="M11.0361 14.5567C12.2714 15.0833 13.3766 15.5287 14.6899 15.883C17.0207 16.5136 19.2311 15.5709 20.3234 13.4872L21.6432 10.8541C21.939 10.2105 22.3128 9.6156 22.7647 9.07273C21.9715 8.78016 21.3149 8.63064 20.4534 8.63064C18.5518 8.63064 16.7606 9.36205 15.4018 10.6948L13.3831 12.6875C12.6647 13.3929 11.878 14.0203 11.0361 14.5567Z" fill="currentColor" />
          <path d="M1.62519 9.74885C1.47889 9.60906 1.23511 9.70983 1.23511 9.91463L1.23834 17.8724C1.23834 18.1 1.34887 18.3112 1.53416 18.4348C3.66663 19.8521 6.15343 20.5998 8.72151 20.5998C11.166 20.5998 13.5488 19.9171 15.6098 18.6266C16.3867 18.139 16.9979 17.7099 17.6512 17.0792C16.6013 17.388 15.4472 17.4075 14.2835 17.0922C9.37815 15.7594 5.20097 13.2044 1.62519 9.74885Z" fill="currentColor" />
        </>
      ) : (
        <path d="M17.3261 8.15754L17.3228 8.15069C17.0394 7.56412 16.6404 6.99422 16.1445 6.47713C14.8786 5.17033 13.0776 4.28741 11.0431 4.05986C10.673 4.01979 10.3047 4 9.94957 4C9.62263 4 9.27922 4.0193 8.91865 4.05952L8.91755 4.05965C6.85659 4.28839 5.03568 5.1673 3.77824 6.46538C3.27725 6.98775 2.87802 7.54898 2.58414 8.14712C2.19144 8.95433 2 9.79185 2 10.6574C2 11.7632 2.33191 12.875 2.98788 13.8581L2.99438 13.8678L2.99435 13.8679C3.38549 14.4616 4.08579 15.2583 4.63143 15.6982L5.61433 16.4906L5.40578 17.366L5.93277 17.0993L6.64094 16.7409L7.40215 16.9656C7.86891 17.1034 8.35651 17.1927 8.91862 17.2554L8.9244 17.256C9.26741 17.2953 9.60902 17.3149 9.94957 17.3149C10.3047 17.3149 10.6736 17.295 11.0448 17.2549C11.5031 17.2043 11.955 17.1167 12.3913 16.9973C12.4856 17.6972 12.8215 18.3342 13.3232 18.8066V18.8073H13.3231C12.6647 19.0154 11.9712 19.1649 11.2619 19.243C10.8205 19.2908 10.3791 19.3149 9.94957 19.3149C9.53203 19.3149 9.11448 19.2908 8.69693 19.243C8.05271 19.1712 7.44429 19.0634 6.83585 18.8838L3.99654 20.3207C3.99654 20.3207 3.70563 20.4523 3.55512 20.4523C3.13758 20.4523 2.85357 20.1671 2.85357 19.748C2.85357 19.4959 2.92051 19.1503 2.98248 18.9077L3.37618 17.2552C2.64845 16.6686 1.81334 15.7106 1.32422 14.9682C0.453338 13.663 0 12.1662 0 10.6574C0 9.48394 0.262458 8.34638 0.787376 7.2687C1.18106 6.46641 1.70599 5.73599 2.33827 5.0774C3.94881 3.41297 6.21551 2.34726 8.69693 2.07185C9.1264 2.02395 9.54395 2 9.94957 2C10.3791 2 10.8205 2.02395 11.2619 2.07185C13.7314 2.34726 15.9742 3.42495 17.5847 5.08937C18.217 5.74795 18.7419 6.49037 19.1237 7.28066C19.5895 8.22633 19.8926 9.2182 19.8928 10.2591C19.7117 10.1958 19.5219 10.1488 19.325 10.1201C18.8263 10.0474 18.3382 10.1001 17.8928 10.2544C17.8917 9.60962 17.7043 8.92531 17.3295 8.16436L17.3261 8.15754ZM21.4505 15.1352L21.4266 15.1113C21.4207 15.1053 21.4116 15.0992 21.4026 15.0932C21.3937 15.0873 21.3848 15.0813 21.3788 15.0754L21.2835 14.9796C20.6632 14.3569 20.2693 13.5906 20.1144 12.7883C20.1144 12.7626 20.1109 12.7369 20.1077 12.7131C20.1049 12.6925 20.1023 12.6732 20.1023 12.6566L20.0666 12.5129C20.0069 12.2854 19.8877 12.0698 19.7086 11.9022C19.1719 11.3633 18.2891 11.3633 17.7521 11.9022C17.2153 12.441 17.2153 13.3271 17.7521 13.866C17.943 14.0575 18.1697 14.1773 18.4203 14.2372C18.4442 14.2491 18.48 14.2491 18.5037 14.2491C18.5157 14.2491 18.5306 14.2521 18.5455 14.2551C18.5604 14.2581 18.5753 14.2611 18.5873 14.2611C19.4104 14.4168 20.1859 14.8119 20.8181 15.4465C20.8659 15.4945 20.9136 15.5424 20.9494 15.5902C21.0806 15.722 21.2835 15.722 21.4147 15.5902C21.546 15.4585 21.546 15.267 21.4505 15.1352ZM20.4001 19.5059L20.3762 19.5298C20.257 19.6257 20.0661 19.6257 19.9228 19.4938C19.7916 19.3622 19.7916 19.1586 19.9228 19.027C19.9701 18.9915 20.0171 18.9443 20.0643 18.8969L20.0643 18.8969L20.0661 18.8951C20.6983 18.2605 21.092 17.4823 21.2471 16.656C21.2471 16.6441 21.2501 16.6291 21.2531 16.6142L21.2531 16.6141C21.256 16.5992 21.259 16.5842 21.259 16.5721C21.259 16.5482 21.259 16.5123 21.271 16.4884C21.3307 16.2369 21.4499 16.0093 21.6408 15.8178C22.1776 15.2789 23.0606 15.2789 23.5973 15.8178C24.1341 16.3567 24.1341 17.2427 23.5973 17.7816C23.4303 17.9612 23.2155 18.081 22.9889 18.1408L22.8458 18.1768C22.8292 18.1768 22.8099 18.1793 22.7893 18.1821C22.7656 18.1853 22.74 18.1888 22.7145 18.1888C21.9151 18.3444 21.1517 18.7395 20.5313 19.3622L20.4359 19.4581C20.4299 19.464 20.4239 19.4731 20.4179 19.4821L20.4179 19.4821C20.4119 19.491 20.406 19.4999 20.4001 19.5059ZM16.0095 18.452L16.0334 18.4759C16.0394 18.4819 16.0484 18.488 16.0574 18.494L16.0575 18.4941C16.0663 18.5 16.0752 18.5059 16.081 18.5118L16.1765 18.6075C16.7969 19.2303 17.1905 19.9966 17.3457 20.7989C17.3457 20.8246 17.3491 20.8503 17.3522 20.8741L17.3523 20.8742C17.355 20.8948 17.3575 20.9141 17.3575 20.9307L17.3935 21.0743C17.453 21.3018 17.5723 21.5174 17.7514 21.685C18.2882 22.2239 19.171 22.2239 19.7078 21.685C20.2447 21.1461 20.2447 20.2601 19.7078 19.7212C19.517 19.5296 19.2903 19.4099 19.0397 19.35C19.0158 19.3381 18.9801 19.3381 18.9562 19.3381C18.9443 19.3381 18.9295 19.3351 18.9146 19.3321C18.8997 19.3291 18.8847 19.3261 18.8727 19.3261C18.0496 19.1705 17.2741 18.7753 16.6419 18.1407C16.5941 18.0927 16.5464 18.0448 16.5107 17.997C16.3794 17.8653 16.1765 17.8653 16.0453 17.997C15.9022 18.1286 15.9022 18.3322 16.0095 18.452ZM17.0476 14.0936L17.0715 14.0696C17.2027 13.9619 17.3936 13.9619 17.5248 14.1055C17.6561 14.2373 17.6561 14.4408 17.5248 14.5725C17.4777 14.608 17.4307 14.6551 17.3835 14.7023L17.3816 14.7043C16.7493 15.3389 16.3557 16.1172 16.2005 16.9435C16.2005 16.9554 16.1976 16.9704 16.1946 16.9854C16.1916 17.0003 16.1887 17.0153 16.1887 17.0272C16.1887 17.0513 16.1887 17.0872 16.1768 17.1111C16.1171 17.3626 15.9978 17.59 15.8069 17.7817C15.2701 18.3206 14.3872 18.3206 13.8504 17.7817C13.3135 17.2428 13.3135 16.3567 13.8504 15.8179C14.0174 15.6383 14.2321 15.5185 14.4588 15.4587L14.602 15.4227C14.6186 15.4227 14.6378 15.4201 14.6583 15.4174L14.6583 15.4174H14.6584C14.6821 15.4142 14.7076 15.4107 14.7332 15.4107C15.5325 15.2551 16.296 14.86 16.9163 14.2373L17.0118 14.1415C17.0179 14.1354 17.0239 14.1263 17.0299 14.1173C17.0358 14.1084 17.0417 14.0995 17.0476 14.0936Z" fill="currentColor" />
      )}
    </svg>
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
  const credentialGen = useRef<PlatformGen>(emptyPlatformGen())
  const controlGen = useRef<PlatformGen>(emptyPlatformGen())
  const sessionRef = useRef<string | null>(null)
  const pollAbortRef = useRef<AbortController | null>(null)
  const expiryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const phaseRef = useRef<'idle' | 'pending' | 'saving'>('idle')
  const committedSetup = useRef(new Set<string>())
  const flowRestoreRef = useRef<FlowRestore | null>(null)
  const restoreQueue = useRef(Promise.resolve())
  const restoreOnLeaveRef = useRef<(id: string) => Promise<void>>(async () => {})
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
  const [credentialBusy, setCredentialBusy] = useState<Record<ImPlatform, boolean>>({
    feishu: false, wecom: false, wecom_callback: false,
  })
  const [setupSession, setSetupSession] = useState<ImSetupSession | null>(null)
  const [setupPlatform, setSetupPlatform] = useState<'feishu' | 'wecom' | null>(null)
  const [setupError, setSetupError] = useState('')
  const [setupBusy, setSetupBusy] = useState(false)
  const [savingSetup, setSavingSetup] = useState(false)
  const [retrySession, setRetrySession] = useState<ImSetupSession | null>(null)
  const [qrUrl, setQrUrl] = useState('')
  const [qrError, setQrError] = useState('')
  const [copied, setCopied] = useState('')
  const [dirError, setDirError] = useState('')
  const [actionError, setActionError] = useState('')
  const [busyAction, setBusyAction] = useState('')
  const [advancedOpen, setAdvancedOpen] = useState(false)

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

  const clearExpiryTimer = () => {
    if (expiryTimerRef.current != null) {
      clearTimeout(expiryTimerRef.current)
      expiryTimerRef.current = null
    }
  }

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
      const saving = phaseRef.current === 'saving'
      setupGen.current += 1
      phaseRef.current = 'idle'
      pollAbortRef.current?.abort()
      if (expiryTimerRef.current != null) {
        clearTimeout(expiryTimerRef.current)
        expiryTimerRef.current = null
      }
      const id = sessionRef.current
      sessionRef.current = null
      if (id) void cancelImSetup(id).catch(() => {})
      if (!saving && id) void restoreOnLeaveRef.current(id)
      unlistenStatus?.()
      unlistenPairing?.()
    }
  }, [hasConfig, refreshPairing, refreshStatus])

  const update = (next: ImConfig) => {
    configRef.current = next
    onChangeRef.current(next)
  }

  const live = (gen: number) => mountedRef.current && gen === setupGen.current

  const expireSetup = (id: string) => {
    if (sessionRef.current !== id || phaseRef.current === 'saving') return
    setupGen.current += 1
    pollAbortRef.current?.abort()
    clearExpiryTimer()
    phaseRef.current = 'idle'
    sessionRef.current = null
    committedSetup.current.delete(id)
    setSavingSetup(false)
    setRetrySession(null)
    setSetupError('')
    setSetupSession((current) => current && current.id === id
      ? { ...current, status: 'expired', message: copy.timeout }
      : current)
    void cancelImSetup(id).catch(() => {})
  }

  const armExpiry = (session: ImSetupSession, gen: number) => {
    clearExpiryTimer()
    const delay = Math.max(0, unixMs(session.expiresAt) - Date.now())
    expiryTimerRef.current = setTimeout(() => {
      expiryTimerRef.current = null
      if (gen !== setupGen.current) return
      expireSetup(session.id)
    }, delay)
  }

  const restoreDraft = async (id: string) => {
    const pending = flowRestoreRef.current
    if (!pending || pending.id !== id || committedSetup.current.has(id)) return
    const now = configRef.current
    if (!now) return
    if (!pending.reverted) {
      if (platformIdentity(now, pending.platform) !== pending.expected) {
        if (flowRestoreRef.current === pending) flowRestoreRef.current = null
        return
      }
      if (flowRestoreRef.current !== pending) return
      update(configWithRestore(now, pending))
      pending.reverted = true
    } else {
      const current = configRef.current
      if (!current || flowRestoreRef.current !== pending || !restoreApplied(current, pending)) {
        if (flowRestoreRef.current === pending) flowRestoreRef.current = null
        return
      }
    }
    if (flowRestoreRef.current !== pending) return
    const flushed = await onFlushRef.current().catch(() => false)
    if (flushed && flowRestoreRef.current === pending) flowRestoreRef.current = null
  }

  const queueRestore = (id: string) => {
    const run = restoreQueue.current.then(() => restoreDraft(id))
    restoreQueue.current = run.then(() => {}, () => {})
    return run
  }
  restoreOnLeaveRef.current = queueRestore

  const commitAuthorized = async (session: ImSetupSession, gen: number) => {
    if (!live(gen)) return
    const expected = stagedIdentity(session)
    const snapshot = configRef.current
    if (!expected || !snapshot) {
      if (live(gen)) setSetupError(copy.identityMissing)
      return
    }
    if (pastExpiry(session) && !committedSetup.current.has(session.id)) {
      expireSetup(session.id)
      return
    }
    clearExpiryTimer()
    phaseRef.current = 'saving'
    setSavingSetup(true)
    setSetupError('')
    setRetrySession(null)
    setSetupSession(session)
    let stored = committedSetup.current.has(session.id)
    try {
      if (!stored) {
        const drafted = applyAuthorizedIdentity(snapshot, session)
        const restore = captureFlowRestore(snapshot, session, expected)
        if (!drafted || !restore) {
          if (live(gen)) setSetupError(copy.identityMissing)
          return
        }
        flowRestoreRef.current = restore
        update(drafted)
        const flushed = await onFlushRef.current()
        if (!live(gen) || !flushed) {
          await queueRestore(session.id)
          if (live(gen)) {
            setSetupError(copy.flushFailed)
            setRetrySession(session)
          }
          return
        }
        const after = configRef.current
        if (!after || platformIdentity(after, session.platform) !== expected) {
          if (flowRestoreRef.current?.id === session.id) flowRestoreRef.current = null
          if (live(gen)) {
            setSetupError(!after || !platformIdentity(after, session.platform) ? copy.identityRequired : copy.identityChanged)
            setRetrySession(session)
          }
          return
        }
        if (!stillUncommittedDraft(after, session.platform, expected)) {
          await queueRestore(session.id)
          if (live(gen)) {
            setSetupError(copy.scanNotSubmitted)
            setRetrySession(session)
          }
          return
        }
        if (!live(gen)) {
          await queueRestore(session.id)
          return
        }
        const committed = await commitImSetup(session.id, expected)
        if (setupStatus(committed) !== 'completed') {
          await queueRestore(session.id)
          if (!live(gen)) return
          setSetupSession(committed)
          setSetupError(committed.message || setupLabel(copy, setupStatus(committed)) || copy.scanNotSubmitted)
          if (setupStatus(committed) !== 'cancelled' && setupStatus(committed) !== 'expired' && setupStatus(committed) !== 'denied') {
            setRetrySession(session)
          }
          return
        }
        stored = true
        committedSetup.current.add(session.id)
        if (flowRestoreRef.current?.id === session.id) flowRestoreRef.current = null
        if (live(gen)) setSetupSession(committed)
      }
      const ready = configRef.current
      if (!ready || platformIdentity(ready, session.platform) !== expected) {
        if (live(gen)) {
          setSetupError(copy.identityChanged)
          setRetrySession(session)
        }
        return
      }
      const wasEnabled = session.platform === 'feishu' ? ready.feishu.enabled
        : session.platform === 'wecom' ? ready.wecom.enabled
          : ready.wecomCallback.enabled
      update(withEnabled(ready, session.platform, true))
      const enabled = await onFlushRef.current()
      if (!enabled) {
        const rolled = configRef.current
        if (rolled && platformIdentity(rolled, session.platform) === expected) {
          update(withEnabled(rolled, session.platform, wasEnabled))
          await onFlushRef.current().catch(() => false)
        }
        if (live(gen)) {
          setSetupError(copy.enableNotSaved)
          setRetrySession(session)
        }
        return
      }
      if (!live(gen)) return
      sessionRef.current = null
      phaseRef.current = 'idle'
      setRetrySession(null)
      setSetupSession(null)
      setSetupError('')
      await refreshStatus()
    } catch (error) {
      if (!stored) {
        await queueRestore(session.id)
        if (!live(gen)) return
        setSetupError(errorText(error, copy.requestFailed))
        setRetrySession(session)
        return
      }
      const rolled = configRef.current
      if (rolled && platformIdentity(rolled, session.platform) === expected) {
        const enabledNow = session.platform === 'feishu' ? rolled.feishu.enabled : rolled.wecom.enabled
        if (enabledNow) {
          update(withEnabled(rolled, session.platform, false))
          await onFlushRef.current().catch(() => false)
        }
      }
      if (!live(gen)) return
      setSetupError(copy.enableNotSaved)
      setRetrySession(session)
    } finally {
      if (gen === setupGen.current) {
        setSavingSetup(false)
        if (phaseRef.current === 'saving') phaseRef.current = 'idle'
      }
    }
  }

  const watchSetup = async (id: string, gen: number, signal: AbortSignal) => {
    try {
      while (live(gen) && !signal.aborted) {
        await sleep(IM_SETUP_POLL_MS, signal)
        if (!live(gen) || signal.aborted) return
        const next = await pollImSetup(id)
        if (!live(gen) || signal.aborted) return
        if (setupStatus(next) === 'pending' && pastExpiry(next)) {
          expireSetup(id)
          return
        }
        if (setupStatus(next) === 'authorized') {
          if (pastExpiry(next)) {
            expireSetup(id)
            return
          }
          await commitAuthorized(next, gen)
          return
        }
        setSetupSession(next)
        if (setupStatus(next) === 'completed') {
          sessionRef.current = null
          clearExpiryTimer()
          phaseRef.current = 'idle'
          setSetupError(copy.scanNotSubmitted)
          return
        }
        if (TERMINAL_SETUP[setupStatus(next)]) {
          sessionRef.current = null
          clearExpiryTimer()
          phaseRef.current = 'idle'
          return
        }
        armExpiry(next, gen)
      }
    } catch (error) {
      if (isAbort(error) || !live(gen)) return
      setSetupError(errorText(error, copy.requestFailed))
    }
  }

  const startSetup = async (platform: ImPlatform) => {
    const current = configRef.current
    if (!current || platform === 'wecom_callback') return
    const saving = phaseRef.current === 'saving'
    const previousId = sessionRef.current
    setupGen.current += 1
    pollAbortRef.current?.abort()
    clearExpiryTimer()
    sessionRef.current = null
    phaseRef.current = 'idle'
    setRetrySession(null)
    setSavingSetup(false)
    if (previousId) void cancelImSetup(previousId).catch(() => {})
    if (!saving && previousId) void queueRestore(previousId)
    const gen = setupGen.current
    setSetupPlatform(platform)
    setSetupError('')
    setQrError('')
    setQrUrl('')
    setSetupBusy(true)
    try {
      const domain = platform === 'feishu' ? current.feishu.domain : null
      const session = await beginImSetup(platform, domain)
      if (!live(gen)) {
        void cancelImSetup(session.id).catch(() => {})
        return
      }
      if (!isSecureSetupUrl(session.url)) {
        setSetupSession({ ...session, status: 'error', message: '' })
        setSetupError(session.url ? copy.badUrl : copy.missingUrl)
        void cancelImSetup(session.id).catch(() => {})
        return
      }
      if (setupStatus(session) === 'pending' && pastExpiry(session)) {
        setSetupSession({ ...session, status: 'expired', message: copy.timeout })
        void cancelImSetup(session.id).catch(() => {})
        return
      }
      sessionRef.current = session.id
      setSetupSession(session)
      if (setupStatus(session) === 'authorized') {
        if (pastExpiry(session)) {
          expireSetup(session.id)
          return
        }
        await commitAuthorized(session, gen)
        return
      }
      if (setupStatus(session) === 'completed') {
        sessionRef.current = null
        setSetupError(copy.scanNotSubmitted)
        return
      }
      if (TERMINAL_SETUP[setupStatus(session)]) {
        sessionRef.current = null
        return
      }
      phaseRef.current = 'pending'
      armExpiry(session, gen)
      const abort = new AbortController()
      pollAbortRef.current = abort
      void watchSetup(session.id, gen, abort.signal)
    } catch (error) {
      if (live(gen)) setSetupError(errorText(error, copy.requestFailed))
    } finally {
      if (live(gen)) setSetupBusy(false)
    }
  }

  const cancelSetup = async () => {
    const saving = phaseRef.current === 'saving'
    setupGen.current += 1
    pollAbortRef.current?.abort()
    clearExpiryTimer()
    phaseRef.current = 'idle'
    setSavingSetup(false)
    setRetrySession(null)
    const id = sessionRef.current
    sessionRef.current = null
    if (!saving && id) void queueRestore(id)
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

  const pendingQrKey = setupSession && setupStatus(setupSession) === 'pending' && isSecureSetupUrl(setupSession.url)
    ? `${setupSession.id}\n${setupSession.url}`
    : ''

  useEffect(() => {
    if (!pendingQrKey) {
      setQrUrl('')
      return
    }
    const url = pendingQrKey.split('\n').slice(1).join('\n')
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
  }, [copy.requestFailed, pendingQrKey])

  const saveCredentials = async (platform: ImPlatform) => {
    const submitted = { ...secretsRef.current[platform] }
    if (!submitted.secret.trim()) {
      patchNote(platform, '', copy.requestFailed)
      return
    }
    const expectedIdentity = configRef.current ? platformIdentity(configRef.current, platform) : ''
    const gen = ++credentialGen.current[platform]
    setCredentialBusy((current) => ({ ...current, [platform]: true }))
    patchNote(platform, '', '')
    try {
      const flushed = await onFlushRef.current()
      if (!mountedRef.current || gen !== credentialGen.current[platform]) return
      if (!flushed) {
        patchNote(platform, '', copy.flushFailed)
        return
      }
      const now = configRef.current ? platformIdentity(configRef.current, platform) : ''
      if (!expectedIdentity || now !== expectedIdentity) {
        patchNote(platform, '', !expectedIdentity || !now ? copy.identityRequired : copy.identityChanged)
        return
      }
      await saveImCredentials(platform, expectedIdentity, submitted)
      if (!mountedRef.current || gen !== credentialGen.current[platform]) return
      const landed = configRef.current ? platformIdentity(configRef.current, platform) : ''
      if (landed !== expectedIdentity) {
        patchNote(platform, '', !expectedIdentity || !landed ? copy.identityRequired : copy.identityChanged)
        return
      }
      const unchanged = sameSecrets(secretsRef.current[platform], submitted)
      if (unchanged) {
        setSecrets((current) => sameSecrets(current[platform], submitted)
          ? { ...current, [platform]: emptySecrets() }
          : current)
      }
      patchNote(platform, unchanged ? copy.saveOk : copy.saveKept, '')
      await refreshStatus()
    } catch (error) {
      if (!mountedRef.current || gen !== credentialGen.current[platform]) return
      patchNote(platform, '', errorText(error, copy.requestFailed))
    } finally {
      if (mountedRef.current && gen === credentialGen.current[platform]) {
        setCredentialBusy((current) => ({ ...current, [platform]: false }))
      }
    }
  }

  const clearCredentials = async (platform: ImPlatform) => {
    const expectedIdentity = configRef.current ? platformIdentity(configRef.current, platform) : ''
    const accepted = await confirmDialog({ message: copy.clearConfirm, confirmLabel: copy.clear, danger: true })
    if (!accepted || !mountedRef.current) return
    const now = configRef.current ? platformIdentity(configRef.current, platform) : ''
    if (!expectedIdentity || now !== expectedIdentity) {
      patchNote(platform, '', !expectedIdentity || !now ? copy.identityRequired : copy.identityChanged)
      return
    }
    const gen = ++credentialGen.current[platform]
    setCredentialBusy((current) => ({ ...current, [platform]: true }))
    patchNote(platform, '', '')
    try {
      await clearImCredentials(platform, expectedIdentity)
      if (!mountedRef.current || gen !== credentialGen.current[platform]) return
      setSecrets((current) => ({ ...current, [platform]: emptySecrets() }))
      patchNote(platform, copy.clearOk, '')
      await refreshStatus()
    } catch (error) {
      if (!mountedRef.current || gen !== credentialGen.current[platform]) return
      patchNote(platform, '', errorText(error, copy.requestFailed))
    } finally {
      if (mountedRef.current && gen === credentialGen.current[platform]) {
        setCredentialBusy((current) => ({ ...current, [platform]: false }))
      }
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

  const disablePlatform = async (platform: ImPlatform) => {
    const snapshot = configRef.current
    if (!snapshot) return
    const wasEnabled = platform === 'feishu' ? snapshot.feishu.enabled
      : platform === 'wecom' ? snapshot.wecom.enabled
        : snapshot.wecomCallback.enabled
    if (!wasEnabled) return
    const gen = ++controlGen.current[platform]
    update(withEnabled(snapshot, platform, false))
    setActionError('')
    try {
      const ok = await onFlushRef.current()
      if (!mountedRef.current || gen !== controlGen.current[platform]) return
      if (!ok) {
        const now = configRef.current
        if (now && platformIdentity(now, platform) === platformIdentity(snapshot, platform)) {
          update(withEnabled(now, platform, true))
        }
        setActionError(copy.flushFailed)
        return
      }
      await refreshStatus()
    } catch (error) {
      if (!mountedRef.current || gen !== controlGen.current[platform]) return
      const now = configRef.current
      if (now && platformIdentity(now, platform) === platformIdentity(snapshot, platform)) {
        update(withEnabled(now, platform, true))
      }
      setActionError(errorText(error, copy.requestFailed))
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
        <p role="status">{row ? `${copy.state[row.state]}${row.message ? ` · ${row.message}` : ''}` : copy.noStatus}</p>
        {row ? <p>{row.credentialsConfigured ? copy.configured : copy.notConfigured}</p> : null}
        {enabled && row && !row.credentialsConfigured ? <p role="alert">{copy.enabledMissing}</p> : null}
        {row?.webhookUrl ? (
          <div className="flex flex-col gap-2">
            {isHttpUrl(row.webhookUrl) ? <a href={row.webhookUrl}>{row.webhookUrl}</a> : <p>{row.webhookUrl}</p>}
            <Button size="sm" onClick={() => void copyWebhook(row.webhookUrl)}>{copy.copyUrl}</Button>
            {copied === row.webhookUrl ? <p>{copy.copied}</p> : null}
          </div>
        ) : null}
        {row?.lastMessageAt != null ? <p>{copy.lastMessage}: {formatUtc(row.lastMessageAt, lang)}</p> : null}
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
          <Button size="sm" variant="primary" aria-label={`${copy.save} ${copy.platform[platform]}`} disabled={credentialBusy[platform]} onClick={() => void saveCredentials(platform)}>{copy.save}</Button>
          <Button size="sm" variant="danger" aria-label={`${copy.clear} ${copy.platform[platform]}`} disabled={credentialBusy[platform]} onClick={() => void clearCredentials(platform)}>{copy.clear}</Button>
        </div>
        {credentialNote[platform] ? <p>{credentialNote[platform]}</p> : null}
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

  const renderQr = (platform: 'feishu' | 'wecom') => {
    const sessionForCard = setupSession && setupSession.platform === platform ? setupSession : null
    const showError = Boolean(setupError) && (sessionForCard != null || (setupSession == null && setupPlatform === platform))
    if (!sessionForCard && !showError) return null
    const statusName = sessionForCard ? setupStatus(sessionForCard) : ''
    const showQr = statusName === 'pending' && qrUrl
    return (
      <section aria-label={copy.setup} className="flex flex-col gap-2 py-2">
        {sessionForCard ? (
          <p>{savingSetup ? copy.savingAuth : `${setupLabel(copy, setupStatus(sessionForCard))}${sessionForCard.message ? ` · ${sessionForCard.message}` : ''}`}</p>
        ) : null}
        {showError ? <p role="alert">{setupError}</p> : null}
        {statusName === 'pending' ? <p className="kv-row-desc">{copy.scanHint[platform]}</p> : null}
        {sessionForCard && statusName === 'pending' && isSecureSetupUrl(sessionForCard.url) ? <a href={sessionForCard.url} target="_blank" rel="noreferrer">{sessionForCard.url}</a> : null}
        {showQr ? <img alt={copy.qrAlt} src={qrUrl} width={220} height={220} /> : null}
        {qrError && sessionForCard ? <p role="alert">{qrError}</p> : null}
        {sessionForCard?.identity ? (
          <p>{copy.botName}: {sessionForCard.identity.botName || '—'} · {copy.owner}: {sessionForCard.identity.ownerId || '—'}</p>
        ) : null}
        {sessionForCard && (statusName === 'pending' || savingSetup) ? <Button size="sm" onClick={() => void cancelSetup()}>{copy.cancelSetup}</Button> : null}
        {retrySession && retrySession.platform === platform ? (
          <Button size="sm" disabled={savingSetup} onClick={() => void commitAuthorized(retrySession, setupGen.current)}>{copy.retrySave}</Button>
        ) : null}
      </section>
    )
  }

  const renderCard = (platform: 'feishu' | 'wecom', title: string, enabled: boolean) => (
    <section aria-label={title} className="kv-group">
      <div className="flex items-center gap-3">
        <PlatformMark platform={platform} />
        <div className="kv-group-title">{title}</div>
      </div>
      {renderStatus(platform, enabled)}
      <div className="flex flex-wrap gap-2">
        <Button
          size="sm"
          variant="primary"
          aria-label={`${copy.scanConnect} ${copy.platform[platform]}`}
          disabled={setupBusy || savingSetup}
          onClick={() => void startSetup(platform)}
        >
          {copy.scanConnect}
        </Button>
        {enabled ? (
          <Button size="sm" aria-label={`${copy.reconnect} ${copy.platform[platform]}`} onClick={() => void reconnect(platform)}>{copy.reconnect}</Button>
        ) : null}
        {enabled ? (
          <Button size="sm" aria-label={`${copy.disable} ${copy.platform[platform]}`} onClick={() => void disablePlatform(platform)}>{copy.disable}</Button>
        ) : null}
      </div>
      {renderQr(platform)}
    </section>
  )

  const pending = requests ?? []
  const approvedUsers = approved ?? []

  return (
    <div className="flex flex-col gap-4">
      <div>
        {copy.pageWarnings.map((warning) => <p key={warning} className="kv-row-desc">{warning}</p>)}
      </div>
      {statusError ? <p role="alert">{copy.statusFailed}: {statusError}</p> : null}
      {statusError ? <Button size="sm" onClick={() => void refreshStatus()}>{copy.retryStatus}</Button> : null}
      {pairingError ? <p role="alert">{copy.pairingFailed}: {pairingError}</p> : null}
      {pairingError ? <Button size="sm" onClick={() => void refreshPairing()}>{copy.retryPairing}</Button> : null}
      {actionError ? <p role="alert">{actionError}</p> : null}

      {renderCard('feishu', copy.feishu, config.feishu.enabled)}
      {renderCard('wecom', copy.wecom, config.wecom.enabled)}

      {pending.length > 0 || approvedUsers.length > 0 ? (
        <section aria-label={copy.pairing} className="kv-group">
          <div className="kv-group-title">{copy.pairing}</div>
          <p className="kv-row-desc">{copy.pairingHint}</p>
          {pending.length > 0 ? <div className="kv-group-title">{copy.pendingTitle}</div> : null}
          {pending.map((request) => {
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
          {approvedUsers.length > 0 ? <div className="kv-group-title">{copy.approvedTitle}</div> : null}
          {approvedUsers.map((user) => {
            const key = `revoke:${user.platform}:${user.identity}:${user.userId}`
            return (
              <div key={key} className="flex flex-col gap-2 py-2">
                <p>{copy.platform[user.platform]} · {user.userName || user.userId} · {user.userId} · {copy.boundId}: {user.identity || '—'}{user.approvedAt ? ` · ${formatUtc(user.approvedAt, lang)}` : ''}</p>
                <Button size="sm" variant="danger" disabled={busyAction === key} onClick={() => void runPairingAction(key, () => revokeImUser(user.platform, user.userId))}>{copy.revoke}</Button>
              </div>
            )
          })}
        </section>
      ) : null}

      <details className="kv-group" open={advancedOpen}>
        <summary
          className="kv-group-title cursor-pointer select-none"
          onClick={(event) => {
            event.preventDefault()
            setAdvancedOpen((open) => !open)
          }}
        >
          {copy.advanced}
        </summary>
        {advancedOpen ? (
          <div className="mt-1 flex flex-col gap-2">
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
                  inheritLabel={copy.inheritModel}
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

            <div>
              <SettingRow label={copy.enabledFeishu}>
                <Toggle ariaLabel={copy.enabledFeishu} checked={config.feishu.enabled} onChange={(enabled) => patchFeishu({ enabled })} />
              </SettingRow>
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
            </div>

            <div>
              <SettingRow label={copy.enabledWecom}>
                <Toggle ariaLabel={copy.enabledWecom} checked={config.wecom.enabled} onChange={(enabled) => patchWecom({ enabled })} />
              </SettingRow>
              <FieldBlock label={copy.botId} htmlFor="im-wecom-bot-id">
                <Input id="im-wecom-bot-id" value={config.wecom.botId} onChange={(botId) => patchWecom({ botId })} />
              </FieldBlock>
              <FieldBlock label={copy.websocketUrl} htmlFor="im-wecom-ws">
                <Input id="im-wecom-ws" value={config.wecom.websocketUrl} onChange={(websocketUrl) => patchWecom({ websocketUrl })} />
              </FieldBlock>
              <AccessEditor idPrefix={copy.wecom} copy={copy} access={config.wecom.access} onChange={(access) => patchWecom({ access })} />
              {renderSecrets('wecom', [{ key: 'secret', label: copy.secretWecom }])}
            </div>

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
              <Button size="sm" aria-label={`${copy.reconnect} ${copy.platform.wecom_callback}`} onClick={() => void reconnect('wecom_callback')}>{copy.reconnect}</Button>
            </section>
          </div>
        ) : null}
      </details>
    </div>
  )
}
