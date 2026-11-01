import { useState } from 'react'
import { PILL_CLASS } from '~/components/entity/property-pill'

export interface EntityTextFieldProps {
  label: string
  value: string
  // Called on blur or Enter with the edited draft, only when it differs from `value`.
  onCommit: (draft: string) => void
  type?: 'text' | 'number'
}

// A pill with an inline input that writes once on blur or Enter, not per keystroke; Escape
// reverts the draft. While focused the draft is the source of truth; outside of focus a
// changed `value` (another client, the MCP agent, a rejected write) is adopted
// during render.
export function EntityTextField({ label, value, onCommit, type = 'text' }: EntityTextFieldProps) {
  const [draft, setDraft] = useState(value)
  const [focused, setFocused] = useState(false)
  const [syncedValue, setSyncedValue] = useState(value)

  if (value !== syncedValue) {
    setSyncedValue(value)
    if (!focused) setDraft(value)
  }

  const commit = () => {
    if (draft !== value) onCommit(draft)
  }

  const handleBlur = () => {
    setFocused(false)
    commit()
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') commit()
    if (e.key === 'Escape') setDraft(value)
  }

  return (
    <label className={PILL_CLASS}>
      <span className="font-medium text-muted-foreground">{label}</span>
      <input
        type={type}
        value={draft}
        placeholder="Empty"
        onFocus={() => setFocused(true)}
        onBlur={handleBlur}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={handleKeyDown}
        className="field-sizing-content min-w-12 bg-transparent outline-none placeholder:text-muted-foreground"
        aria-label={label}
      />
    </label>
  )
}
