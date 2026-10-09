import { invoke as coreInvoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

import type {
  CredentialInput, FeishuDomain, ImApprovedUser, ImPairingRequest, ImPlatform, ImSetupSession, ImStatus,
} from '../generated/im'
export type {
  ConnectionState, CredentialInput, DmPolicy, FeishuConfig, FeishuConnectionMode, FeishuDomain, GroupPolicy,
  ImAccessConfig, ImAgentConfig, ImApprovedUser, ImConfig, ImPairingRequest, ImPlatform, ImSetupIdentity,
  ImSetupSession, ImSetupState, ImStatus, ImWebhookConfig, WecomCallbackConfig, WecomConfig,
} from '../generated/im'

export const IM_STATUS_EVENT = 'im-status-changed'
export const IM_PAIRING_EVENT = 'im-pairing-changed'

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await coreInvoke<T>(command, args)
  } catch (error) {
    const preview = typeof window !== 'undefined' && !('__TAURI_INTERNALS__' in window)
    if (preview && error instanceof TypeError) {
      throw new Error('当前是浏览器预览，此功能需要在桌面应用中使用')
    }
    throw error
  }
}

function asArray<T>(value: T[] | null | undefined, invalid: string): T[] {
  if (!Array.isArray(value)) throw new Error(invalid)
  return value
}

export function getImStatus(): Promise<ImStatus[]> {
  return invoke<ImStatus[]>('im_get_status').then((rows) => asArray(rows, 'IM 状态响应无效'))
}

export function saveImCredentials(
  platform: ImPlatform,
  expectedIdentity: string,
  credentials: CredentialInput,
): Promise<void> {
  return invoke<void>('im_save_credentials', { platform, expectedIdentity, credentials })
}

export function clearImCredentials(platform: ImPlatform, expectedIdentity: string): Promise<void> {
  return invoke<void>('im_clear_credentials', { platform, expectedIdentity })
}

export function reconnectIm(platform: ImPlatform): Promise<void> {
  return invoke<void>('im_reconnect', { platform })
}

export function listImPairingRequests(): Promise<ImPairingRequest[]> {
  return invoke<ImPairingRequest[]>('im_list_pairing_requests').then((rows) => asArray(rows, 'IM 配对请求响应无效'))
}

export function listImApprovedUsers(): Promise<ImApprovedUser[]> {
  return invoke<ImApprovedUser[]>('im_list_approved_users').then((rows) => asArray(rows, 'IM 已批准用户响应无效'))
}

export function approveImPairing(platform: ImPlatform, code: string): Promise<void> {
  return invoke<void>('im_approve_pairing', { platform, code })
}

export function denyImPairing(platform: ImPlatform, code: string): Promise<void> {
  return invoke<void>('im_deny_pairing', { platform, code })
}

export function revokeImUser(platform: ImPlatform, userId: string): Promise<void> {
  return invoke<void>('im_revoke_user', { platform, userId })
}

export function beginImSetup(platform: ImPlatform, domain?: FeishuDomain | null): Promise<ImSetupSession> {
  return invoke<ImSetupSession>('im_begin_setup', { platform, domain: domain ?? null })
}

export function pollImSetup(id: string): Promise<ImSetupSession> {
  return invoke<ImSetupSession>('im_poll_setup', { id })
}

export function commitImSetup(id: string, expectedIdentity: string): Promise<ImSetupSession> {
  return invoke<ImSetupSession>('im_commit_setup', { id, expectedIdentity })
}

export function cancelImSetup(id: string): Promise<void> {
  return invoke<void>('im_cancel_setup', { id })
}

export function subscribeImStatus(onChange: () => void): Promise<() => void> {
  return listen(IM_STATUS_EVENT, () => { onChange() })
}

export function subscribeImPairing(onChange: () => void): Promise<() => void> {
  return listen(IM_PAIRING_EVENT, () => { onChange() })
}
