import { useEffect, useId, useRef, useState } from 'react'
import type { Lang } from '../../components/i18n'
import type { DmPolicy, GroupPolicy, ImAccessConfig } from '../../api/im'
import { FieldBlock, Select, TextArea } from '../public/controls'
import { Button, IconButton } from '../../components/Button'
import { X } from 'lucide-react'

const DM_POLICIES: DmPolicy[] = ['pairing', 'allowlist', 'open', 'disabled']
const GROUP_POLICIES: GroupPolicy[] = ['allowlist', 'open', 'disabled']

type Draft = {
  dmPolicy: DmPolicy
  groupPolicy: GroupPolicy
  allowedUsers: string
  allowedGroups: string
  groupUsers: Record<string, string>
}

function listText(values: string[]): string {
  return values.join('\n')
}

function parseImIdList(text: string): string[] {
  const seen = new Set<string>()
  const ids: string[] = []
  for (const part of text.split(/[\n,]/)) {
    const id = part.trim()
    if (!id || seen.has(id)) continue
    seen.add(id)
    ids.push(id)
  }
  return ids
}

function accessDraft(access: ImAccessConfig): Draft {
  const groups = [...access.allowedGroups]
  const listed = new Set(groups)
  for (const id of Object.keys(access.groupUsers)) {
    if (listed.has(id) || access.groupUsers[id].length === 0) continue
    listed.add(id)
    groups.push(id)
  }
  return {
    dmPolicy: access.dmPolicy,
    groupPolicy: access.groupPolicy,
    allowedUsers: listText(access.allowedUsers),
    allowedGroups: listText(groups),
    groupUsers: Object.fromEntries(Object.entries(access.groupUsers).map(([id, users]) => [id, listText(users)])),
  }
}

function draftToAccess(draft: Draft): ImAccessConfig {
  const allowedGroups = parseImIdList(draft.allowedGroups)
  const groupUsers: Record<string, string[]> = {}
  for (const id of allowedGroups) {
    const users = parseImIdList(draft.groupUsers[id] ?? '')
    if (users.length > 0) groupUsers[id] = users
  }
  return {
    dmPolicy: draft.dmPolicy,
    groupPolicy: draft.groupPolicy,
    allowedUsers: parseImIdList(draft.allowedUsers),
    allowedGroups,
    groupUsers,
  }
}

function accessDraftIsOpen(draft: Draft): boolean {
  if (draft.dmPolicy === 'open' || draft.groupPolicy === 'open') return true
  const lists = [draft.allowedUsers, draft.allowedGroups, ...Object.values(draft.groupUsers)]
  return lists.some((text) => parseImIdList(text).includes('*'))
}

function accessCopy(lang: Lang, platform: 'feishu' | 'wecom') {
  const zh = lang === 'zh'
  const name = platform === 'feishu' ? (zh ? '飞书' : 'Feishu') : (zh ? '企业微信' : 'WeCom')
  const policy = {
    pairing: zh ? '配对' : 'Pairing',
    allowlist: zh ? '允许列表' : 'Allowlist',
    open: zh ? '开放' : 'Open',
    disabled: zh ? '关闭' : 'Disabled',
  }
  return {
    dialogTitle: zh ? `${name} · 高级设置` : `${name} · Advanced settings`,
    close: zh ? '关闭高级设置' : 'Close advanced settings',
    cancel: zh ? '取消' : 'Cancel',
    dm: zh ? `${name}私聊策略` : `${name} direct message policy`,
    group: zh ? `${name}群策略` : `${name} group policy`,
    dmHint: zh ? '默认配对。允许列表只接受下面的用户。' : 'Pairing is the default. Allowlist accepts only the users below.',
    groupHint: zh ? '默认允许列表。未列出的群不会接入。' : 'Allowlist is the default. Groups that are not listed are not admitted.',
    users: zh ? `${name}允许的用户` : `${name} allowed users`,
    usersHint: zh ? '每行一个用户 ID，也可以用逗号分隔。未单独限制群内用户时，群消息也会用这份列表。' : 'One user ID per line, or separate them with commas. Groups without their own user list use this list.',
    groups: zh ? `${name}允许的群` : `${name} allowed groups`,
    groupsHint: zh ? '每行一个群 ID，也可以用逗号分隔。' : 'One group ID per line, or separate them with commas.',
    groupUsers: (id: string) => zh ? `${name}群内用户 ${id}` : `${name} users in ${id}`,
    groupUsersHint: zh ? '留空则这个群不再单独限制发言人。' : 'Leave this empty to stop restricting senders in this group.',
    save: zh ? `保存群访问 ${name}` : `Save group access ${name}`,
    saving: zh ? '正在保存群访问…' : 'Saving group access…',
    failed: zh ? '群访问没有保存。' : 'Group access was not saved.',
    openWarning: zh
      ? '开放或通配（*）会让命中的用户自动执行本机工具，不再弹出桌面确认。'
      : 'Open access or a wildcard (*) lets matching users run local tools automatically, without desktop confirmation.',
    openLabel: zh ? '开放访问说明' : 'Open access warning',
    policy,
  }
}

export function ImAccessEditor({
  platform,
  lang,
  access,
  onSave,
  onClose,
}: {
  platform: 'feishu' | 'wecom'
  lang: Lang
  access: ImAccessConfig
  onSave: (access: ImAccessConfig) => Promise<boolean>
  onClose: () => void
}) {
  const copy = accessCopy(lang, platform)
  const [draft, setDraft] = useState(() => accessDraft(access))
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const dirty = useRef(false)
  const titleId = useId()
  const dialogRef = useRef<HTMLDialogElement>(null)
  const headingRef = useRef<HTMLHeadingElement>(null)

  useEffect(() => {
    const dialog = dialogRef.current
    if (!dialog) return
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    dialog.showModal()
    headingRef.current?.focus()
    return () => {
      dialog.close()
      if (previous?.isConnected) previous.focus()
    }
  }, [])
  const accessKey = JSON.stringify(access)

  useEffect(() => {
    if (dirty.current) return
    setDraft(accessDraft(access))
  }, [access, accessKey])

  const change = (patch: Partial<Draft>) => {
    dirty.current = true
    setError('')
    setDraft((current) => ({ ...current, ...patch }))
  }

  const save = async () => {
    setSaving(true)
    setError('')
    const next = draftToAccess(draft)
    let ok = false
    try {
      ok = await onSave(next)
    } catch {
      ok = false
    } finally {
      setSaving(false)
    }
    if (ok) {
      dirty.current = false
      onClose()
    } else setError(copy.failed)
  }

  const rows = parseImIdList(draft.allowedGroups)
  const open = accessDraftIsOpen(draft)
  const dmOptions = DM_POLICIES.map((value) => ({ value, label: copy.policy[value] }))
  const groupOptions = GROUP_POLICIES.map((value) => ({ value, label: copy.policy[value] }))
  return (
    <dialog
      ref={dialogRef}
      className="kv-modal im-config-dialog"
      aria-labelledby={titleId}
      aria-modal="true"
      data-tauri-drag-region="false"
      onKeyDown={(event) => event.stopPropagation()}
      onCancel={(event) => {
        event.preventDefault()
        if (!saving) onClose()
      }}
    >
      <div className="im-config-dialog-header">
        <h2 id={titleId} ref={headingRef} tabIndex={-1} className="im-heading">{copy.dialogTitle}</h2>
        <IconButton label={copy.close} variant="ghost" size="sm" disabled={saving} onClick={onClose}>
          <X size={16} aria-hidden="true" />
        </IconButton>
      </div>
      <div className="im-config-dialog-body im-form custom-scrollbar">
        <FieldBlock label={copy.dm} description={copy.dmHint}>
          <Select ariaLabel={copy.dm} disabled={saving} value={draft.dmPolicy} options={dmOptions} onChange={(value) => change({ dmPolicy: value as DmPolicy })} />
        </FieldBlock>
        <FieldBlock label={copy.group} description={copy.groupHint}>
          <Select ariaLabel={copy.group} disabled={saving} value={draft.groupPolicy} options={groupOptions} onChange={(value) => change({ groupPolicy: value as GroupPolicy })} />
        </FieldBlock>
        <FieldBlock label={copy.users} description={copy.usersHint}>
          <TextArea aria-label={copy.users} disabled={saving} rows={3} value={draft.allowedUsers} onChange={(value) => change({ allowedUsers: value })} spellCheck={false} />
        </FieldBlock>
        <FieldBlock label={copy.groups} description={copy.groupsHint}>
          <TextArea aria-label={copy.groups} disabled={saving} rows={3} value={draft.allowedGroups} onChange={(value) => change({ allowedGroups: value })} spellCheck={false} />
        </FieldBlock>
        {rows.map((id) => (
          <FieldBlock key={id} label={copy.groupUsers(id)} description={copy.groupUsersHint}>
            <TextArea
              aria-label={copy.groupUsers(id)}
              disabled={saving}
              rows={2}
              value={draft.groupUsers[id] ?? ''}
              onChange={(value) => change({ groupUsers: { ...draft.groupUsers, [id]: value } })}
              spellCheck={false}
            />
          </FieldBlock>
        ))}
        {open ? (
          <div className="im-security-notice" role="region" aria-label={copy.openLabel}>
            <p>{copy.openWarning}</p>
          </div>
        ) : null}
        {error ? <p role="alert">{error}</p> : null}
      </div>
      <div className="im-config-dialog-footer">
        <Button disabled={saving} onClick={onClose}>{copy.cancel}</Button>
        <Button variant="primary" disabled={saving} onClick={() => void save()}>{saving ? copy.saving : copy.save}</Button>
      </div>
    </dialog>
  )
}
