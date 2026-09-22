export function RequirementComposer({ label, value, onChange, disabled, placeholder }: {
  label: string; value: string; onChange: (next: string) => void; disabled?: boolean; placeholder: string;
}) {
  return <div className="is-req"><div className="is-req-bar"><span>{label}</span></div>
    <textarea className="kv-textarea custom-scrollbar" aria-label={label} disabled={disabled} value={value} onChange={e => onChange(e.target.value)} placeholder={placeholder} />
  </div>
}
