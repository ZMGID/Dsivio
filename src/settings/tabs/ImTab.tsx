import { useCallback, useEffect, useId, useRef, useState } from 'react'
import { QrCode, ShieldAlert, X } from 'lucide-react'
import { Button, IconButton } from '../../components/Button'
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
  subscribeImPairing,
  subscribeImStatus,
} from '../../api/im'
import type {
  ConnectionState, FeishuConfig, FeishuDomain, ImAccessConfig, ImApprovedUser, ImConfig, ImPairingRequest, ImPlatform,
  ImSetupSession, ImStatus, WecomConfig,
} from '../../api/im'
import { Select } from '../public/controls'
import { authorizationQrDataUrl } from './imQr'
import { ImAccessEditor } from './ImAccessEditor'
import { ImManualConnect } from './ImManualConnect'

export const IM_SETUP_POLL_MS = 1000

const TERMINAL_SETUP: Record<string, true> = {
  completed: true, denied: true, expired: true, cancelled: true, error: true,
}

type PlatformGen = Record<ImPlatform, number>

function emptyPlatformGen(): PlatformGen {
  return { feishu: 0, wecom: 0, wecom_callback: 0 }
}

function unixMs(value: number): number {
  if (!Number.isFinite(value)) return 0
  return value > 0 && value < 1_000_000_000_000 ? value * 1000 : value
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
      zh ? '连接后，在对应应用中向机器人发消息即可使用。请保持 Dsivio 运行。' : 'Once connected, send a message to the bot in the app. Keep Dsivio running.',
    ],
    missing: zh ? '无法读取连接配置，请重新打开设置。' : 'Could not load connection settings. Please reopen settings.',
    feishu: zh ? '飞书 / Lark' : 'Feishu / Lark',
    wecom: zh ? '企业微信机器人' : 'WeCom bot',
    flushFailed: zh ? '无法保存连接，请重试。' : 'Could not save the connection. Please retry.',
    enableNotSaved: zh ? '授权已保存，但连接未能启动。请重试。' : 'Authorization was saved, but the connection could not start. Please retry.',
    identityRequired: zh ? '机器人信息已丢失，请重新扫码。' : 'Bot information is missing. Please scan again.',
    identityChanged: zh ? '机器人配置已变化，请重新扫码。' : 'The bot configuration changed. Please scan again.',
    requestFailed: zh ? '请求失败' : 'Request failed',
    scanConnect: zh ? '扫码连接' : 'Connect with QR',
    scanAgain: zh ? '重新扫码' : 'Scan again',
    scanFailedManual: zh ? '扫码没有完成。请手动配置机器人。' : 'The scan did not finish. Configure the bot manually.',
    useManual: zh ? '改用手动配置' : 'Use manual setup',
    advanced: zh ? '高级设置' : 'Advanced settings',
    domain: zh ? '飞书域' : 'Feishu domain',
    domainFeishu: zh ? '飞书' : 'Feishu',
    domainLark: 'Lark',
    scanHint: {
      feishu: zh ? '打开飞书或 Lark，扫描二维码并在客户端里确认授权。' : 'Open Feishu or Lark, scan this QR code, and confirm the authorization in the app.',
      wecom: zh ? '打开企业微信，扫描二维码并在客户端里确认授权。' : 'Open WeCom, scan this QR code, and confirm the authorization in the app.',
    } satisfies Record<'feishu' | 'wecom', string>,
    qrAlt: zh ? '授权二维码' : 'Authorization QR code',
    close: zh ? '关闭扫码窗口' : 'Close QR setup',
    preparingQr: zh ? '正在生成二维码…' : 'Preparing the QR code…',
    retrySave: zh ? '重试保存授权' : 'Retry saving authorization',
    savingAuth: zh ? '正在完成连接…' : 'Finishing the connection…',
    missingUrl: zh ? '无法获取授权二维码，请重试。' : 'Could not get the authorization QR code. Please retry.',
    badUrl: zh ? '授权链接不是 https，已停止展示。' : 'The authorization link is not https, so it is not shown.',
    identityMissing: zh ? '未获取到机器人信息，请重新扫码。' : 'Could not get the bot information. Please scan again.',
    scanNotSubmitted: zh ? '这次扫码没有提交。' : 'This scan was not submitted.',
    timeout: zh ? '扫码已超时。' : 'QR setup timed out.',
    reconnect: zh ? '重新连接' : 'Reconnect',
    disable: zh ? '断开连接' : 'Disconnect',
    noStatus: zh ? '连接状态暂不可用' : 'Connection status is unavailable',
    statusFailed: zh ? '状态刷新失败' : 'Status refresh failed',
    retryStatus: zh ? '重试刷新状态' : 'Retry status refresh',
    pairing: zh ? '配对' : 'Pairing',
    pairingHint: zh ? '仅批准你认识的人。批准后，对方即可通过机器人使用助手。' : 'Only approve people you recognize. Approved users can use the assistant through the bot.',
    approve: zh ? '批准' : 'Approve',
    deny: zh ? '拒绝' : 'Deny',
    revoke: zh ? '撤销' : 'Revoke',
    pendingTitle: zh ? '待处理请求' : 'Pending requests',
    approvedTitle: zh ? '已批准用户' : 'Approved users',
    refreshFailed: zh ? '操作已提交，但列表刷新失败。' : 'The action was submitted, but refreshing the list failed.',
    pairingFailed: zh ? '配对列表刷新失败' : 'Pairing list refresh failed',
    retryPairing: zh ? '重试刷新配对' : 'Retry pairing refresh',
    state: {
      disabled: zh ? '未连接' : 'Not connected',
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
  }
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

export function ImTab({ lang, config, onChange, onFlush }: {
  lang: Lang
  config: ImConfig | null
  onChange: (config: ImConfig) => void
  onFlush: () => Promise<boolean>
}) {
  const copy = imCopy(lang)
  const requestFailedRef = useRef(copy.requestFailed)
  requestFailedRef.current = copy.requestFailed
  const setupTitleId = useId()
  const configRef = useRef(config)
  configRef.current = config
  const onChangeRef = useRef(onChange)
  onChangeRef.current = onChange
  const onFlushRef = useRef(onFlush)
  onFlushRef.current = onFlush
  const mountedRef = useRef(true)
  const setupGen = useRef(0)
  const cancelledThroughGen = useRef(0)
  const statusGen = useRef(0)
  const pairingGen = useRef(0)
  const controlGen = useRef<PlatformGen>(emptyPlatformGen())
  const sessionRef = useRef<string | null>(null)
  const pollAbortRef = useRef<AbortController | null>(null)
  const expiryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const phaseRef = useRef<'idle' | 'pending' | 'saving'>('idle')
  const committedSetup = useRef(new Set<string>())
  const flowRestoreRef = useRef<FlowRestore | null>(null)
  const restoreQueue = useRef(Promise.resolve())
  const restoreOnLeaveRef = useRef<(id: string) => Promise<void>>(async () => {})
  const setupHeadingRef = useRef<HTMLHeadingElement>(null)
  const setupDialogRef = useRef<HTMLDialogElement>(null)
  const feishuButtonRef = useRef<HTMLButtonElement>(null)
  const wecomButtonRef = useRef<HTMLButtonElement>(null)
  const returnPlatformRef = useRef<'feishu' | 'wecom' | null>(null)

  const [status, setStatus] = useState<ImStatus[] | null>(null)
  const [statusError, setStatusError] = useState('')
  const [requests, setRequests] = useState<ImPairingRequest[] | null>(null)
  const [approved, setApproved] = useState<ImApprovedUser[] | null>(null)
  const [pairingError, setPairingError] = useState('')
  const [setupSession, setSetupSession] = useState<ImSetupSession | null>(null)
  const [setupPlatform, setSetupPlatform] = useState<'feishu' | 'wecom' | null>(null)
  const [setupError, setSetupError] = useState('')
  const [setupBusy, setSetupBusy] = useState(false)
  const [savingSetup, setSavingSetup] = useState(false)
  const [retrySession, setRetrySession] = useState<ImSetupSession | null>(null)
  const [qrUrl, setQrUrl] = useState('')
  const [qrError, setQrError] = useState('')
  const [actionError, setActionError] = useState('')
  const [busyAction, setBusyAction] = useState('')
  const configuredDomain = config?.feishu.domain ?? 'feishu'
  const feishuDomainRef = useRef<FeishuDomain>(configuredDomain)
  const accessGen = useRef({ feishu: 0, wecom: 0 })
  const [feishuDomain, setFeishuDomain] = useState<FeishuDomain>(configuredDomain)
  const [manualOpen, setManualOpen] = useState<{ feishu: boolean, wecom: boolean }>({ feishu: false, wecom: false })
  const [manualSuspend, setManualSuspend] = useState<{ feishu: number, wecom: number }>({ feishu: 0, wecom: 0 })
  const [manualBusy, setManualBusy] = useState({ feishu: false, wecom: false })
  const [accessPlatform, setAccessPlatform] = useState<'feishu' | 'wecom' | null>(null)

  useEffect(() => {
    feishuDomainRef.current = configuredDomain
    setFeishuDomain(configuredDomain)
  }, [configuredDomain])

  useEffect(() => {
    const dialog = setupDialogRef.current
    if (!setupPlatform || !dialog) return
    if (!dialog.open) dialog.showModal()
    setupHeadingRef.current?.focus()
    return () => {
      dialog.close()
      const target = returnPlatformRef.current === 'feishu' ? feishuButtonRef : wecomButtonRef
      target.current?.focus()
    }
  }, [setupPlatform])

  const refreshStatus = useCallback(async () => {
    const gen = ++statusGen.current
    try {
      const rows = await getImStatus()
      if (!mountedRef.current || gen !== statusGen.current) return
      setStatus(rows)
      setStatusError('')
    } catch (error) {
      if (!mountedRef.current || gen !== statusGen.current) return
      setStatusError(errorText(error, requestFailedRef.current))
    }
  }, [])

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
      setPairingError(errorText(error, requestFailedRef.current))
      return 'error' as const
    }
  }, [])

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
          setSetupError(committed.message || copy.setupState[setupStatus(committed)] || copy.scanNotSubmitted)
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
      // A completed credential commit survives navigation, but explicit close revokes activation.
      if (gen <= cancelledThroughGen.current) return
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
      setSetupPlatform(null)
      void refreshStatus()
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

  const chooseDomain = (value: FeishuDomain) => {
    feishuDomainRef.current = value
    setFeishuDomain(value)
  }

  const startSetup = async (platform: ImPlatform, domainOverride?: FeishuDomain) => {
    const current = configRef.current
    if (!current || platform === 'wecom_callback') return
    if (platform === 'feishu' || platform === 'wecom') {
      setManualSuspend((currentSuspend) => ({ ...currentSuspend, [platform]: currentSuspend[platform] + 1 }))
    }
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
    returnPlatformRef.current = platform
    setSetupPlatform(platform)
    setSetupSession(null)
    setSetupError('')
    setQrError('')
    setQrUrl('')
    setSetupBusy(true)
    setupHeadingRef.current?.focus()
    try {
      const domain = platform === 'feishu' ? (domainOverride ?? feishuDomainRef.current) : null
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
    cancelledThroughGen.current = setupGen.current
    const gen = ++setupGen.current
    pollAbortRef.current?.abort()
    clearExpiryTimer()
    phaseRef.current = 'idle'
    setSetupBusy(false)
    setSavingSetup(false)
    setRetrySession(null)
    setSetupSession(null)
    setSetupPlatform(null)
    setSetupError('')
    setQrUrl('')
    setQrError('')
    setActionError('')
    const id = sessionRef.current
    sessionRef.current = null
    if (!saving && id) void queueRestore(id)
    if (id) {
      try {
        await cancelImSetup(id)
      } catch (error) {
        if (live(gen)) setActionError(errorText(error, copy.requestFailed))
      }
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
        setQrError(requestFailedRef.current)
        setQrUrl('')
        return
      }
      setQrUrl(dataUrl)
    }).catch((error) => {
      if (!alive || gen !== setupGen.current || !mountedRef.current) return
      setQrUrl('')
      setQrError(errorText(error, requestFailedRef.current))
    })
    return () => { alive = false }
  }, [pendingQrKey])

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
  const withAccess = (source: ImConfig, platform: 'feishu' | 'wecom', access: ImAccessConfig): ImConfig => (
    platform === 'feishu'
      ? { ...source, feishu: { ...source.feishu, access } }
      : { ...source, wecom: { ...source.wecom, access } }
  )

  const saveAccess = async (platform: 'feishu' | 'wecom', next: ImAccessConfig) => {
    const before = configRef.current
    if (!before) return false
    const previous = platform === 'feishu' ? before.feishu.access : before.wecom.access
    const gen = ++accessGen.current[platform]
    update(withAccess(before, platform, next))
    let ok = false
    try {
      ok = await onFlushRef.current()
    } catch {
      ok = false
    }
    if (!mountedRef.current || gen !== accessGen.current[platform]) return ok
    if (!ok) {
      const now = configRef.current
      if (now) update(withAccess(now, platform, previous))
      return false
    }
    return true
  }

  const openManual = (platform: 'feishu' | 'wecom') => {
    setManualOpen((currentOpen) => ({ ...currentOpen, [platform]: true }))
    void cancelSetup()
  }

  if (!config) {
    return (
      <section>
        <p role="alert">{copy.missing}</p>
      </section>
    )
  }

  const statusOf = (platform: ImPlatform) => status?.find((row) => row.platform === platform) ?? null

  const renderStatus = (platform: ImPlatform) => {
    const row = statusOf(platform)
    return (
      <div className="im-connection-status">
        <div role="status" className="im-status-line">
          {row ? (
            <span className={`kv-tag status ${row.state === 'connected' ? 'ok' : row.state === 'error' ? 'danger' : row.state === 'connecting' || row.state === 'retrying' ? 'accent' : ''}`}>
              {copy.state[row.state]}
            </span>
          ) : <span className="kv-row-desc">{copy.noStatus}</span>}
          {row?.state === 'error' && row.message ? <span className="im-status-message">{row.message}</span> : null}
        </div>
      </div>
    )
  }

  const renderCard = (platform: 'feishu' | 'wecom', title: string, enabled: boolean) => {
    const row = statusOf(platform)
    const connected = row?.state === 'connected'
    const scanLabel = connected ? copy.scanAgain : copy.scanConnect
    return (
      <section aria-label={title} className="kv-group">
        <div className="im-connection-content">
          <div className="im-connection-heading">
            <PlatformMark platform={platform} />
            <h2 className="im-heading">{title}</h2>
          </div>
          {renderStatus(platform)}
          <div className="im-actions">
            <Button
              ref={platform === 'feishu' ? feishuButtonRef : wecomButtonRef}
              variant={connected ? 'default' : 'primary'}
              aria-label={`${scanLabel} ${copy.platform[platform]}`}
              disabled={setupBusy || savingSetup || manualBusy[platform]}
              onClick={() => void startSetup(platform)}
            >
              <QrCode size={14} aria-hidden="true" />
              {scanLabel}
            </Button>
            {enabled && row?.state === 'error' && row.credentialsConfigured ? (
              <Button size="sm" disabled={manualBusy[platform]} aria-label={`${copy.reconnect} ${copy.platform[platform]}`} onClick={() => void reconnect(platform)}>{copy.reconnect}</Button>
            ) : null}
            {enabled ? (
              <Button size="sm" disabled={manualBusy[platform]} aria-label={`${copy.disable} ${copy.platform[platform]}`} onClick={() => void disablePlatform(platform)}>{copy.disable}</Button>
            ) : null}
          </div>
          <div className="im-actions">
            <ImManualConnect
              platform={platform}
              lang={lang}
              open={manualOpen[platform]}
              onOpenChange={(next) => setManualOpen((currentOpen) => ({ ...currentOpen, [platform]: next }))}
              domain={feishuDomain}
              onDomainChange={chooseDomain}
              showDomain={setupPlatform !== 'feishu'}
              suspendEpoch={manualSuspend[platform]}
              configuredIdentity={platform === 'feishu' ? config.feishu.appId : config.wecom.botId}
              readConfig={() => configRef.current}
              onChange={(next) => update(next)}
              onFlush={() => onFlushRef.current()}
              onSettled={() => { void refreshStatus() }}
              onBusyChange={(busy) => setManualBusy((currentBusy) => ({ ...currentBusy, [platform]: busy }))}
            />
            <Button
              size="sm"
              aria-label={`${copy.advanced} ${copy.platform[platform]}`}
              aria-haspopup="dialog"
              disabled={setupBusy || savingSetup || manualBusy[platform]}
              onClick={(event) => {
                event.currentTarget.focus()
                setAccessPlatform(platform)
              }}
            >
              {copy.advanced}
            </Button>
          </div>
          {accessPlatform === platform ? (
            <ImAccessEditor
              platform={platform}
              lang={lang}
              access={platform === 'feishu' ? config.feishu.access : config.wecom.access}
              onSave={(next) => saveAccess(platform, next)}
              onClose={() => setAccessPlatform(null)}
            />
          ) : null}
        </div>
      </section>
    )
  }

  const session = setupSession?.platform === setupPlatform ? setupSession : null
  const setupState = session ? setupStatus(session) : ''
  const pendingSetup = setupState === 'pending'
  const loadingQr = setupBusy || (pendingSetup && !qrUrl && !qrError)
  const pending = requests ?? []
  const approvedUsers = approved ?? []

  return (
    <div className="im-settings">
      <div className="im-security-notice">
        <ShieldAlert size={17} aria-hidden="true" />
        <p>{copy.pageWarnings[0]}</p>
      </div>
      <div className="im-operating-notes">
        {copy.pageWarnings.slice(1).map((warning) => <p key={warning}>{warning}</p>)}
      </div>
      {statusError ? <p role="alert">{copy.statusFailed}: {statusError}</p> : null}
      {statusError ? <Button size="sm" onClick={() => void refreshStatus()}>{copy.retryStatus}</Button> : null}
      {pairingError ? <p role="alert">{copy.pairingFailed}: {pairingError}</p> : null}
      {pairingError ? <Button size="sm" onClick={() => void refreshPairing()}>{copy.retryPairing}</Button> : null}
      {actionError ? <p role="alert">{actionError}</p> : null}

      <div className="im-connections">
        {renderCard('feishu', copy.feishu, config.feishu.enabled)}
        {renderCard('wecom', copy.wecom, config.wecom.enabled)}
      </div>
      {pending.length > 0 || approvedUsers.length > 0 ? (
        <section aria-label={copy.pairing} className="kv-group">
          <div className="kv-group-title">{copy.pairing}</div>
          <p className="kv-row-desc">{copy.pairingHint}</p>
          {pending.length > 0 ? <div className="kv-group-title">{copy.pendingTitle}</div> : null}
          {pending.map((request) => {
            const key = `${request.platform}:${request.code}`
            return (
              <div key={key} className="flex flex-col gap-2 py-2">
                <p>{copy.platform[request.platform]} · {request.userName || request.userId} · {request.code}</p>
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
                <p>{copy.platform[user.platform]} · {user.userName || user.userId}</p>
                <Button size="sm" variant="danger" disabled={busyAction === key} onClick={() => void runPairingAction(key, () => revokeImUser(user.platform, user.userId))}>{copy.revoke}</Button>
              </div>
            )
          })}
        </section>
      ) : null}
      {setupPlatform ? (
        <dialog
          ref={setupDialogRef}
          className="kv-modal im-qr-dialog custom-scrollbar"
          aria-modal="true"
          aria-labelledby={setupTitleId}
          aria-describedby={`${setupTitleId}-hint`}
          data-tauri-drag-region="false"
          onKeyDown={(event) => event.stopPropagation()}
          onCancel={(event) => {
            event.preventDefault()
            void cancelSetup()
          }}
        >
          <div className="im-qr-dialog-header">
            <div className="im-connection-heading">
              <PlatformMark platform={setupPlatform} />
              <h2 id={setupTitleId} ref={setupHeadingRef} tabIndex={-1} className="im-heading">
                {setupPlatform === 'feishu' ? copy.feishu : copy.wecom}
              </h2>
            </div>
            <IconButton label={copy.close} variant="ghost" size="sm" onClick={() => void cancelSetup()}>
              <X size={16} aria-hidden="true" />
            </IconButton>
          </div>
          <div className="im-qr-dialog-body">
            <p id={`${setupTitleId}-hint`} className="kv-row-desc">{copy.scanHint[setupPlatform]}</p>
            {setupPlatform === 'feishu' ? (
              <div className="im-form">
                <Select
                  ariaLabel={copy.domain}
                  value={feishuDomain}
                  options={[{ value: 'feishu', label: copy.domainFeishu }, { value: 'lark', label: copy.domainLark }]}
                  onChange={(value) => {
                    const next = value === 'lark' ? 'lark' : 'feishu'
                    chooseDomain(next)
                    if (!savingSetup) void startSetup('feishu', next)
                  }}
                />
              </div>
            ) : null}
            {pendingSetup && qrUrl ? <img alt={copy.qrAlt} src={qrUrl} width={260} height={260} /> : null}
            {savingSetup || loadingQr || session ? (
              <p role="status">
                {savingSetup ? copy.savingAuth : loadingQr ? copy.preparingQr : session ? copy.setupState[setupStatus(session)] : null}
              </p>
            ) : null}
            {(setupState === 'error' || setupState === 'denied') && session?.message ? <p role="alert">{session.message}</p> : null}
            {setupError ? <p role="alert">{setupError}</p> : null}
            {qrError ? <p role="alert">{qrError}</p> : null}
            {session?.identity?.botName ? <p>{session.identity.botName}</p> : null}
            {pendingSetup && session && isSecureSetupUrl(session.url) ? (
              <a href={session.url} target="_blank" rel="noreferrer">{copy.zh ? '打开授权页面' : 'Open authorization page'}</a>
            ) : null}
            {!savingSetup && !setupBusy && (setupError || qrError || (session && !pendingSetup)) ? (
              <>
                <p>{copy.scanFailedManual}</p>
                <Button size="sm" onClick={() => openManual(setupPlatform)}>{copy.useManual}</Button>
              </>
            ) : null}
            {retrySession ? (
              <Button variant="primary" disabled={savingSetup} onClick={() => void commitAuthorized(retrySession, setupGen.current)}>{copy.retrySave}</Button>
            ) : !setupBusy && !savingSetup && (setupError || qrError || (session && !pendingSetup)) ? (
              <Button variant="primary" aria-label={`${copy.scanAgain} ${copy.platform[setupPlatform]}`} onClick={() => void startSetup(setupPlatform)}>{copy.scanAgain}</Button>
            ) : null}
          </div>
        </dialog>
      ) : null}
    </div>
  )
}
