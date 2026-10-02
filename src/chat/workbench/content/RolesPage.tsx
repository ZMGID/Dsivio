import { useEffect, useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { convertFileSrc } from '@tauri-apps/api/core'
import { Button } from '../../../components/Button'
import { useLang, useT } from '../../../components/i18n'
import { Input, TextArea } from '../../../settings/public/controls'
import type { Role, RoleInlineImage } from '../../../generated/roles'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { useRoles } from './useRoles'
import { STORE_IMAGE_EXTENSIONS, useFileDrop } from '../useFileDrop'
import './roleTemplates.css'

const portraits = [
  ['01', '东亚女性 · 28 岁', 'East Asian woman · 28'],
  ['02', '西非男性 · 36 岁', 'West African man · 36'],
  ['03', '南亚女性 · 47 岁', 'South Asian woman · 47'],
  ['04', '拉美男性 · 24 岁', 'Latin American man · 24'],
  ['05', '中东女性 · 33 岁', 'Middle Eastern woman · 33'],
  ['06', '东亚男性 · 68 岁', 'East Asian man · 68'],
  ['07', '非洲裔女性 · 22 岁', 'Black woman · 22'],
  ['08', '欧洲女性 · 41 岁', 'European woman · 41'],
  ['09', '东南亚男性 · 29 岁', 'Southeast Asian man · 29'],
  ['10', '拉美原住民女性 · 53 岁', 'Indigenous Latin American woman · 53'],
  ['11', '欧洲男性 · 61 岁', 'European man · 61'],
  ['12', '非洲裔中性形象 · 27 岁', 'Black nonbinary adult · 27'],
  ['13', '东亚男性 · 34 岁', 'East Asian man · 34'],
  ['14', '中东男性 · 45 岁', 'Middle Eastern man · 45'],
  ['15', '南亚女性 · 72 岁', 'South Asian woman · 72'],
] as const

type Draft = {
  id?: string
  revision?: number
  name: string
  description: string
  images: string[]
  imports: string[]
  inline: RoleInlineImage[]
}

function emptyDraft(): Draft {
  return { name: '', description: '', images: [], imports: [], inline: [] }
}

function draftFromRole(role: Role): Draft {
  return {
    id: role.id,
    revision: role.revision,
    name: role.name,
    description: role.description,
    images: role.images,
    imports: [],
    inline: [],
  }
}

function previewSrc(path: string): string {
  try {
    return convertFileSrc(path)
  } catch {
    return path
  }
}

function inlineSrc(image: RoleInlineImage): string {
  const ext = image.name.split('.').pop()?.toLowerCase()
  const mime = ext === 'png' ? 'image/png' : ext === 'webp' ? 'image/webp' : 'image/jpeg'
  return `data:${mime};base64,${image.base64}`
}

async function fileToInline(id: string): Promise<RoleInlineImage> {
  const response = await fetch(`/role-templates/avatar-${id}.webp`)
  if (!response.ok) throw new Error(`avatar-${id}`)
  const bytes = new Uint8Array(await response.arrayBuffer())
  let binary = ''
  for (const byte of bytes) binary += String.fromCharCode(byte)
  return { name: `avatar-${id}.webp`, base64: btoa(binary) }
}

export function RolesPage() {
  const t = useT()
  const en = useLang() === 'en'
  const { roles, error, refresh, save, remove } = useRoles()
  const [draft, setDraft] = useState<Draft>(emptyDraft)
  const [notice, setNotice] = useState('')
  const [saving, setSaving] = useState(false)
  const alive = useRef(true)
  useEffect(() => {
    alive.current = true
    return () => { alive.current = false }
  }, [])

  async function copyPreset(id: string, label: string) {
    try {
      const image = await fileToInline(id)
      if (!alive.current) return
      setDraft((current) => ({
        ...current,
        name: current.name.trim() ? current.name : label,
        inline: current.inline.some((item) => item.name === image.name) ? current.inline : [...current.inline, image],
      }))
      setNotice('')
    } catch (err) {
      if (!alive.current) return
      setNotice(err instanceof Error ? err.message : String(err))
    }
  }

  function addImports(paths: string[]) {
    setDraft((current) => ({
      ...current,
      imports: [...current.imports, ...paths.filter((path) => !current.imports.includes(path) && !current.images.includes(path))],
    }))
  }

  async function pickFiles() {
    const picked = await open({
      multiple: true,
      directory: false,
      filters: [{ name: 'Image', extensions: [...STORE_IMAGE_EXTENSIONS] }],
    })
    if (!alive.current || picked == null) return
    addImports(Array.isArray(picked) ? picked : [picked])
  }

  const zone = useRef<HTMLDivElement>(null)
  const over = useFileDrop(zone, STORE_IMAGE_EXTENSIONS, (accepted, rejected) => {
    if (accepted.length > 0) addImports(accepted)
    else if (rejected.length > 0) setNotice(`${t.workbenchDropUnsupported}${STORE_IMAGE_EXTENSIONS.join(' / ')}`)
  }, saving)

  async function onSave() {
    setSaving(true)
    setNotice('')
    try {
      const saved = await save({
        id: draft.id ?? null,
        revision: draft.revision ?? null,
        name: draft.name,
        description: draft.description,
        images: [...draft.images, ...draft.imports],
        inlineImages: draft.inline,
      })
      if (!alive.current) return
      setDraft(draftFromRole(saved))
      setNotice(t.workbenchRolesSaved)
    } catch (err) {
      if (!alive.current) return
      setNotice(err instanceof Error ? err.message : String(err))
      void refresh()
    } finally {
      if (alive.current) setSaving(false)
    }
  }

  async function onDelete(role: Role) {
    setNotice('')
    try {
      await remove(role.id)
      if (!alive.current) return
      if (draft.id === role.id) setDraft(emptyDraft())
    } catch (err) {
      if (!alive.current) return
      setNotice(err instanceof Error ? err.message : String(err))
    }
  }

  const thumbs = [
    ...draft.images.map((path) => ({ key: path, src: previewSrc(path), drop: () => setDraft((current) => ({ ...current, images: current.images.filter((item) => item !== path) })) })),
    ...draft.imports.map((path) => ({ key: path, src: previewSrc(path), drop: () => setDraft((current) => ({ ...current, imports: current.imports.filter((item) => item !== path) })) })),
    ...draft.inline.map((image) => ({ key: image.name, src: inlineSrc(image), drop: () => setDraft((current) => ({ ...current, inline: current.inline.filter((item) => item.name !== image.name) })) })),
  ]

  return (
    <WorkbenchPage
      className="content-role-templates"
      crumb={t.workbenchGroupContent}
      title={t.workbenchNavRoles}
      subtitle={t.workbenchRolesSubtitle}
      error={error}
      actions={<Button size="sm" onClick={() => { setDraft(emptyDraft()); setNotice('') }}>{t.workbenchRolesAdd}</Button>}
    >
      {notice ? <p className="workbench-inline-note">{notice}</p> : null}
      <div className="workbench-split">
        <WorkbenchCard title={t.workbenchRolesMine} extra={<Button size="sm" onClick={() => void refresh()}>{t.workbenchRefresh}</Button>}>
          {roles.length === 0 ? (
            <WorkbenchEmpty title={t.workbenchRolesEmpty}>{t.workbenchRolesEmptyHint}</WorkbenchEmpty>
          ) : (
            <ul className="role-saved-list">
              {roles.map((role) => (
                <li key={role.id}>
                  <button type="button" className="role-saved-open" onClick={() => setDraft(draftFromRole(role))}>
                    {role.images[0] ? <img src={previewSrc(role.images[0])} alt="" /> : null}
                    <span>{role.name}</span>
                  </button>
                  <Button size="sm" aria-label={`${t.workbenchRolesDelete} ${role.name}`} onClick={() => void onDelete(role)}>{t.workbenchRolesDelete}</Button>
                </li>
              ))}
            </ul>
          )}
        </WorkbenchCard>
        <WorkbenchCard title={t.workbenchRolesEdit}>
          <div ref={zone} className={`workbench-drop-zone${over ? ' is-drop-over' : ''}`}>
          <label className="workbench-field">
            <span>{t.workbenchRolesName}</span>
            <Input aria-label={t.workbenchRolesName} value={draft.name} onChange={(name) => setDraft((current) => ({ ...current, name }))} />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchRolesDesc}</span>
            <TextArea value={draft.description} onChange={(description) => setDraft((current) => ({ ...current, description }))} rows={4} placeholder={t.workbenchRolesDescHint} />
          </label>
          <div className="role-draft-images">
            {thumbs.map((thumb) => (
              <figure key={thumb.key}>
                <img src={thumb.src} alt="" />
                <Button size="sm" onClick={thumb.drop}>{t.workbenchUploadRemove}</Button>
              </figure>
            ))}
          </div>
          <div className="workbench-cta-row">
            <Button size="sm" onClick={() => void pickFiles()}>{t.workbenchRolesPickImages}</Button>
            <Button variant="primary" disabled={saving || !draft.name.trim()} onClick={() => void onSave()}>{t.workbenchRolesSave}</Button>
          </div>
          </div>
        </WorkbenchCard>
      </div>
      <WorkbenchCard title={t.workbenchRolesPresets}>
        <div className="role-template-grid">
          {portraits.map(([id, zh, english], index) => {
            const label = en ? english : zh
            return (
              <figure key={id} className="role-template-card">
                <img src={`/role-templates/avatar-${id}.webp`} alt={label} loading={index < 6 ? 'eager' : 'lazy'} decoding="async" />
                <figcaption>{label}</figcaption>
                <div className="role-template-action">
                  <Button size="sm" onClick={() => void copyPreset(id, label)}>{t.workbenchRolesUsePreset}</Button>
                </div>
              </figure>
            )
          })}
        </div>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
