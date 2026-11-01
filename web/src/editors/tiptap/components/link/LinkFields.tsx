import type { RefObject } from 'react'
import { CheckIcon } from '@phosphor-icons/react'
import { Button } from '~/components/ui/button'
import { Input } from '~/components/ui/input'

export interface LinkFieldsProps {
  label: string
  url: string
  onLabelChange: (label: string) => void
  onUrlChange: (url: string) => void
  onSave: () => void
  onEscape: () => void
  urlRef?: RefObject<HTMLInputElement | null>
  autoFocus?: boolean
}

// A link's text and address. Until the text is changed by hand it follows the address,
// so pasting an address alone links it as itself.
export function LinkFields({
  label,
  url,
  onLabelChange,
  onUrlChange,
  onSave,
  onEscape,
  urlRef,
  autoFocus,
}: LinkFieldsProps) {
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') onSave()
    if (e.key === 'Escape') onEscape()
  }

  const handleUrlChange = (next: string) => {
    if (label === '' || label === url) onLabelChange(next)
    onUrlChange(next)
  }

  return (
    <div className="flex w-72 flex-col gap-2">
      <label className="flex flex-col gap-1 text-xs font-semibold text-muted-foreground">
        Text
        <Input
          value={label}
          onChange={(e) => onLabelChange(e.target.value)}
          onKeyDown={handleKeyDown}
          className="text-foreground"
        />
      </label>
      <label className="flex flex-col gap-1 text-xs font-semibold text-muted-foreground">
        Link
        <div className="flex items-center gap-1.5">
          <Input
            ref={urlRef}
            value={url}
            onChange={(e) => handleUrlChange(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="https://"
            autoFocus={autoFocus}
            className="text-foreground"
          />
          <Button size="icon" onClick={onSave} aria-label="Save link">
            <CheckIcon weight="bold" />
          </Button>
        </div>
      </label>
    </div>
  )
}
