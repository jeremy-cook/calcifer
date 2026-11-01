import { TextAaIcon } from '@phosphor-icons/react'
import type { Editor } from '@tiptap/react'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { toggleVariants } from '~/components/ui/toggle'
import { cn } from '~/lib/utils'
import { HIGHLIGHT_SWATCHES, TEXT_SWATCHES } from '../formatting-options'

interface TextColorPopoverProps {
  editor: Editor
  color: string
  highlight: string
  disabled?: boolean
}

// One "Aa" button for both text colour and highlight. Its underline shows the
// colour in use: the text colour, else the highlight.
export function TextColorPopover({ editor, color, highlight, disabled }: TextColorPopoverProps) {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          className={cn(toggleVariants({ variant: 'tool' }), 'relative')}
          aria-label="Text colour and highlight"
          aria-pressed={Boolean(color || highlight)}
          disabled={disabled}
        >
          <TextAaIcon />
          <span
            className="absolute inset-x-2 bottom-1 h-[3px] rounded-full bg-border"
            style={color || highlight ? { backgroundColor: color || highlight } : undefined}
          />
        </button>
      </PopoverTrigger>
      <PopoverContent className="flex w-auto flex-col gap-2 p-2.5" align="start">
        <SwatchRow
          label="Text"
          swatches={TEXT_SWATCHES}
          value={color}
          onPick={(c) => editor.chain().focus().setColor(c).run()}
          onClear={() => editor.chain().focus().unsetColor().run()}
        />
        <SwatchRow
          label="Highlight"
          swatches={HIGHLIGHT_SWATCHES}
          value={highlight}
          onPick={(c) => editor.chain().focus().setHighlight({ color: c }).run()}
          onClear={() => editor.chain().focus().unsetHighlight().run()}
        />
      </PopoverContent>
    </Popover>
  )
}

interface SwatchRowProps {
  label: string
  swatches: readonly string[]
  value: string
  onPick: (color: string) => void
  onClear: () => void
}

// "None", the palette, then the browser's picker for anything else.
function SwatchRow({ label, swatches, value, onPick, onClear }: SwatchRowProps) {
  const SWATCH_CLASS =
    'size-5 shrink-0 rounded-full outline-none ring-offset-2 ring-offset-popover hover:ring-2 hover:ring-ring focus-visible:ring-2 focus-visible:ring-ring'

  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-[0.7rem] font-bold tracking-widest text-muted-foreground uppercase">{label}</span>
      <div className="flex items-center gap-1.5">
        <button
          type="button"
          className={cn(SWATCH_CLASS, 'border border-border', !value && 'ring-2 ring-foreground')}
          aria-label={`No ${label.toLowerCase()} colour`}
          onClick={onClear}
        />
        {swatches.map((swatch) => (
          <button
            key={swatch}
            type="button"
            className={cn(SWATCH_CLASS, value === swatch && 'ring-2 ring-foreground')}
            style={{ backgroundColor: swatch }}
            aria-label={swatch}
            onClick={() => onPick(swatch)}
          />
        ))}
        <input
          type="color"
          value={value.slice(0, 7) || '#000000'}
          onChange={(e) => onPick(e.target.value)}
          className="size-5 shrink-0 cursor-pointer rounded-full border border-border bg-transparent p-0 [&::-webkit-color-swatch]:rounded-full [&::-webkit-color-swatch]:border-0 [&::-webkit-color-swatch-wrapper]:p-0"
          aria-label={`Custom ${label.toLowerCase()} colour`}
        />
      </div>
    </div>
  )
}
