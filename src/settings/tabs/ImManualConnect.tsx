import { useEffect, useId, useRef, useState } from 'react'
import type { Lang } from '../../components/i18n'
import { saveImCredentials } from '../../api/im'
import type { CredentialInput, FeishuConfig, FeishuDomain, ImConfig, WecomConfig } from '../../api/im'
import { FieldBlock, Input, Select } from '../public/controls'
import { Button, IconButton } from '../../components/Button'
import { X } from 'lucide-react'

type Platform = 'feishu' | 'wecom'
type FeishuIdentity = Pick<FeishuConfig, 'appId' | 'domain' | 'connectionMode' | 'enabled'>
type WecomIdentity = Pick<WecomConfig, 'botId' | 'enabled'>
type IdentityRestore = { feishu?: FeishuIdentity, wecom?: WecomIdentity }

function errorText(error: unknown, fallback: string): string {
  if (typeof error === 'string' && error.trim()) return error
  if (error instanceof Error && error.message.trim()) return error.message
  return fallback
}

function blankCredential(secret: string): CredentialInput {
  return { secret, encryptKey: '', verificationToken: '', token: '', encodingAesKey: '' }
}

function configuredIdentity(config: ImConfig, platform: Platform): string {
  return platform === 'feishu' ? config.feishu.appId : config.wecom.botId
}

function readRestore(config: ImConfig, platform: Platform): IdentityRestore {
  if (platform === 'feishu') {
    return {
      feishu: {
        appId: config.feishu.appId,
        domain: config.feishu.domain,
        connectionMode: config.feishu.connectionMode,
        enabled: config.feishu.enabled,
      },
    }
  }
  return { wecom: { botId: config.wecom.botId, enabled: config.wecom.enabled } }
}

function applyRestore(config: ImConfig, restore: IdentityRestore): ImConfig {
  if (restore.feishu) return { ...config, feishu: { ...config.feishu, ...restore.feishu } }
  if (restore.wecom) return { ...config, wecom: { ...config.wecom, ...restore.wecom } }
  return config
}

/** Disabled identity only. Webhook, access, agent, and the other platform stay untouched. */
function stageDisabled(config: ImConfig, platform: Platform, expected: string, domain: FeishuDomain): ImConfig {
  if (platform === 'feishu') {
    return {
      ...config,
      feishu: { ...config.feishu, appId: expected, domain, connectionMode: 'websocket', enabled: false },
    }
  }
  return { ...config, wecom: { ...config.wecom, botId: expected, enabled: false } }
}

function withEnabled(config: ImConfig, platform: Platform, enabled: boolean): ImConfig {
  if (platform === 'feishu') return { ...config, feishu: { ...config.feishu, enabled } }
  return { ...config, wecom: { ...config.wecom, enabled } }
}

function manualCopy(lang: Lang, platform: Platform) {
  const zh = lang === 'zh'
  const name = platform === 'feishu' ? (zh ? '飞书' : 'Feishu') : (zh ? '企业微信' : 'WeCom')
  return {
    toggle: zh ? `手动配置 ${name}` : `Manual setup ${name}`,
    hint: zh ? '扫码失败时，在这里填写机器人信息并连接。密钥保存在本机 credentials.json，不使用钥匙串。' : 'If the scan fails, enter the bot here and connect. The secret is saved locally in credentials.json, not in the system keychain.',
    appId: 'App ID',
    botId: 'Bot ID',
    domain: zh ? '飞书域' : 'Feishu domain',
    feishu: zh ? '飞书' : 'Feishu',
    lark: 'Lark',
    secret: zh ? '密钥' : 'Secret',
    secretHint: zh ? '不会显示已保存的密钥。请填写要使用的新密钥。' : 'Saved secrets are not shown. Enter the new secret to use.',
    connect: zh ? `保存并连接 ${name}` : `Save and connect ${name}`,
    connecting: zh ? '正在保存并连接…' : 'Saving and connecting…',
    cancel: zh ? `取消手动连接 ${name}` : `Cancel manual setup ${name}`,
    close: zh ? '关闭手动配置' : 'Close manual setup',
    missing: zh ? '请填写机器人 ID 和密钥。' : 'Enter the bot ID and secret.',
    identityFailed: zh ? '机器人身份没有保存，密钥未写入。' : 'The bot identity was not saved, so the secret was not stored.',
    secretFailed: zh ? '密钥没有保存，连接未启用。' : 'The secret was not stored, so the connection was not enabled.',
    enableFailed: zh ? '密钥已保存，但连接未能启用。' : 'The secret was stored, but the connection was not enabled.',
    identityChanged: zh ? '机器人配置已变化，密钥未写入。' : 'The bot configuration changed, so the secret was not stored.',
    requestFailed: zh ? '请求失败' : 'Request failed',
  }
}

export function ImManualConnect({
  platform,
  lang,
  open,
  onOpenChange,
  domain,
  onDomainChange,
  showDomain,
  suspendEpoch,
  configuredIdentity: configured,
  readConfig,
  onChange,
  onFlush,
  onSettled,
  onBusyChange,
}: {
  platform: Platform
  lang: Lang
  open: boolean
  onOpenChange: (open: boolean) => void
  domain: FeishuDomain
  onDomainChange: (domain: FeishuDomain) => void
  showDomain: boolean
  suspendEpoch: number
  configuredIdentity: string
  readConfig: () => ImConfig | null
  onChange: (config: ImConfig) => void
  onFlush: () => Promise<boolean>
  onSettled: () => void
  onBusyChange: (busy: boolean) => void
}) {
  const copy = manualCopy(lang, platform)
  const panelId = useId()
  const dialogRef = useRef<HTMLDialogElement>(null)
  const headingRef = useRef<HTMLHeadingElement>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const [identity, setIdentity] = useState(configured)
  const [secret, setSecret] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const genRef = useRef(0)
  const txRef = useRef(0)
  const phaseRef = useRef<'idle' | 'running'>('idle')
  const cancelRef = useRef<(disposing?: boolean) => void>(() => {})
  const suspendSeen = useRef(suspendEpoch)
  const wasOpen = useRef(open)
  const mountedRef = useRef(true)
  const pendingEnableRef = useRef<{ expected: string, enabled: boolean } | null>(null)

  useEffect(() => {
    if (!open) {
      setSecret('')
      setError('')
      return
    }
    const dialog = dialogRef.current
    if (!dialog) return
    const trigger = triggerRef.current
    dialog.showModal()
    headingRef.current?.focus()
    return () => {
      dialog.close()
      if (trigger?.isConnected) trigger.focus()
    }
  }, [open])

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      cancelRef.current(true)
    }
  }, [])

  useEffect(() => {
    if (phaseRef.current === 'running') return
    setIdentity(configured)
  }, [configured, busy, open])

  const revert = async (restore: IdentityRestore, expected: string) => {
    if (!mountedRef.current) return
    const now = readConfig()
    if (!now || configuredIdentity(now, platform) !== expected) return
    onChange(applyRestore(now, restore))
    await onFlush().catch(() => false)
  }

  const cancel = (disposing = false) => {
    genRef.current += 1
    if (!disposing) return
    const restore = pendingEnableRef.current
    pendingEnableRef.current = null
    const now = readConfig()
    if (restore && now && configuredIdentity(now, platform) === restore.expected) {
      onChange(withEnabled(now, platform, restore.enabled))
      void onFlush().catch(() => false)
    }
  }
  cancelRef.current = cancel

  useEffect(() => {
    if (suspendSeen.current === suspendEpoch) return
    suspendSeen.current = suspendEpoch
    cancelRef.current()
  }, [suspendEpoch])

  useEffect(() => {
    if (wasOpen.current && !open) cancelRef.current()
    wasOpen.current = open
  }, [open])

  const close = () => {
    cancel()
    setSecret('')
    setError('')
    onOpenChange(false)
  }
  const connect = async () => {
    if (phaseRef.current === 'running') return
    const expected = identity.trim()
    const secretValue = secret.trim()
    if (!expected || !secretValue) {
      setError(copy.missing)
      return
    }
    const snapshot = readConfig()
    if (!snapshot) {
      setError(copy.requestFailed)
      return
    }
    const gen = ++genRef.current
    const tx = ++txRef.current
    const previous = readRestore(snapshot, platform)
    phaseRef.current = 'running'
    setBusy(true)
    onBusyChange(true)
    setError('')
    onChange(stageDisabled(snapshot, platform, expected, domain))
    let secretSaved = false
    const current = () => gen === genRef.current
    const latest = () => tx === txRef.current
    try {
      const flushed = await onFlush()
      if (!current()) {
        if (mountedRef.current && latest() && !secretSaved) {
          if (flushed) await revert(previous, expected)
          else {
            const now = readConfig()
            if (now && configuredIdentity(now, platform) === expected) onChange(applyRestore(now, previous))
          }
        }
        return
      }
      if (!flushed) {
        const now = readConfig()
        if (now && configuredIdentity(now, platform) === expected) onChange(applyRestore(now, previous))
        setError(copy.identityFailed)
        return
      }
      const staged = readConfig()
      if (!staged || configuredIdentity(staged, platform) !== expected) {
        if (current()) setError(copy.identityChanged)
        return
      }
      if (!current()) {
        if (latest()) await revert(previous, expected)
        return
      }
      await saveImCredentials(platform, expected, blankCredential(secretValue))
      secretSaved = true
      if (!current()) return
      const ready = readConfig()
      if (!ready || configuredIdentity(ready, platform) !== expected) {
        if (current()) setError(copy.identityChanged)
        return
      }
      const wasEnabled = platform === 'feishu' ? ready.feishu.enabled : ready.wecom.enabled
      pendingEnableRef.current = { expected, enabled: wasEnabled }
      onChange(withEnabled(ready, platform, true))
      const enabled = await onFlush()
      if (!current()) {
        if (mountedRef.current && latest()) {
          const now = readConfig()
          if (now && configuredIdentity(now, platform) === expected) {
            onChange(withEnabled(now, platform, wasEnabled))
            await onFlush().catch(() => false)
          }
        }
        return
      }
      if (!enabled) {
        const now = readConfig()
        if (now && configuredIdentity(now, platform) === expected) {
          onChange(withEnabled(now, platform, wasEnabled))
          await onFlush().catch(() => false)
        }
        setError(copy.enableFailed)
        return
      }
      if (mountedRef.current) {
        setSecret('')
        onOpenChange(false)
      }
      onSettled()
    } catch (caught) {
      if (!secretSaved && latest()) await revert(previous, expected)
      if (current() && mountedRef.current) setError(errorText(caught, copy.secretFailed))
    } finally {
      if (latest()) {
        pendingEnableRef.current = null
        phaseRef.current = 'idle'
        if (mountedRef.current) setBusy(false)
        if (mountedRef.current) onBusyChange(false)
      }
    }
  }

  return (
    <>
      <Button
        ref={triggerRef}
        size="sm"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        disabled={busy}
        onClick={() => onOpenChange(true)}
      >
        {copy.toggle}
      </Button>
      {open ? (
        <dialog
          id={panelId}
          ref={dialogRef}
          className="kv-modal im-config-dialog"
          aria-labelledby={`${panelId}-title`}
          aria-describedby={`${panelId}-hint`}
          aria-modal="true"
          data-tauri-drag-region="false"
          onKeyDown={(event) => event.stopPropagation()}
          onCancel={(event) => {
            event.preventDefault()
            close()
          }}
        >
          <div className="im-config-dialog-header">
            <h2 id={`${panelId}-title`} ref={headingRef} tabIndex={-1} className="im-heading">{copy.toggle}</h2>
            <IconButton label={copy.close} variant="ghost" size="sm" onClick={close}>
              <X size={16} aria-hidden="true" />
            </IconButton>
          </div>
          <div className="im-config-dialog-body im-form custom-scrollbar">
            <p id={`${panelId}-hint`} className="kv-row-desc">{copy.hint}</p>
            <FieldBlock label={platform === 'feishu' ? copy.appId : copy.botId}>
              <Input
                aria-label={platform === 'feishu' ? copy.appId : copy.botId}
                value={identity}
                disabled={busy}
                onChange={setIdentity}
                autoComplete="off"
                spellCheck={false}
              />
            </FieldBlock>
            {platform === 'feishu' && showDomain ? (
              <FieldBlock label={copy.domain}>
                <Select
                  ariaLabel={copy.domain}
                  value={domain}
                  disabled={busy}
                  options={[{ value: 'feishu', label: copy.feishu }, { value: 'lark', label: copy.lark }]}
                  onChange={(value) => onDomainChange(value === 'lark' ? 'lark' : 'feishu')}
                />
              </FieldBlock>
            ) : null}
            <FieldBlock label={copy.secret} description={copy.secretHint}>
              <Input
                aria-label={copy.secret}
                type="password"
                value={secret}
                disabled={busy}
                onChange={setSecret}
                autoComplete="new-password"
                spellCheck={false}
              />
            </FieldBlock>
            {error ? <p role="alert">{error}</p> : null}
          </div>
          <div className="im-config-dialog-footer">
            <Button onClick={close}>{copy.cancel}</Button>
            <Button variant="primary" disabled={busy} onClick={() => void connect()}>
              {busy ? copy.connecting : copy.connect}
            </Button>
          </div>
        </dialog>
      ) : null}
    </>
  )
}
