import { CheckIcon, DotsThreeIcon, EraserIcon, TextSubscriptIcon, TextSuperscriptIcon } from '@phosphor-icons/react'
import type { Editor } from '@tiptap/react'
import { Popover, PopoverContent, PopoverTrigger } from '~/components/ui/popover'
import { ToggleGroup, ToggleGroupItem } from '~/components/ui/toggle-group'
import { toggleVariants } from '~/components/ui/toggle'
import { ALIGNMENTS, type Alignment } from '../formatting-options'

interface MorePopoverProps {
  editor: Editor
  alignment: Alignment
  isSubscript: boolean
  isSuperscript: boolean
  // Inline code or a code block: marks don't apply.
  disableInline: boolean
}

// The tools used less often: alignment, subscript and superscript, clear formatting.
export function MorePopover({ editor, alignment, isSubscript, isSuperscript, disableInline }: MorePopoverProps) {
  const rows = [
    {
      label: 'Subscript',
      icon: TextSubscriptIcon,
      shortcut: '⌘,',
      active: isSubscript,
      disabled: disableInline,
      run: () => editor.chain().focus().toggleSubscript().run(),
    },
    {
      label: 'Superscript',
      icon: TextSuperscriptIcon,
      shortcut: '⌘.',
      active: isSuperscript,
      disabled: disableInline,
      run: () => editor.chain().focus().toggleSuperscript().run(),
    },
  ] as const

  const ROW_CLASS =
    'flex w-full items-center gap-2.5 rounded px-2.5 py-1.5 text-left text-sm hover:bg-accent disabled:pointer-events-none disabled:opacity-40 [&_svg]:size-4 [&_svg]:shrink-0'

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button type="button" className={toggleVariants({ variant: 'tool' })} aria-label="More formatting">
          <DotsThreeIcon weight="bold" />
        </button>
      </PopoverTrigger>
      <PopoverContent className="flex w-52 flex-col gap-0.5 p-1.5" align="start">
        <span className="px-2.5 pt-1 text-[0.7rem] font-bold tracking-widest text-muted-foreground uppercase">
          Align
        </span>
        <ToggleGroup
          type="single"
          variant="segmented"
          spacing={0.5}
          value={alignment}
          onValueChange={(value) => value && editor.chain().focus().setTextAlign(value).run()}
          className="m-1"
        >
          {ALIGNMENTS.map(({ value, icon, label }) => (
            <ToggleGroupItem key={value} value={value} aria-label={label} className="px-2.5">
              {icon}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <div className="-mx-1.5 my-1 h-px bg-border" />
        {rows.map(({ label, icon: Icon, shortcut, active, disabled, run }) => (
          <button key={label} type="button" className={ROW_CLASS} disabled={disabled} onClick={run}>
            <Icon className="text-muted-foreground" />
            {label}
            {active && <CheckIcon className="text-primary" />}
            <kbd className="ml-auto font-sans text-xs text-muted-foreground">{shortcut}</kbd>
          </button>
        ))}
        <div className="-mx-1.5 my-1 h-px bg-border" />
        <button type="button" className={ROW_CLASS} onClick={() => editor.chain().focus().unsetAllMarks().run()}>
          <EraserIcon className="text-muted-foreground" />
          Clear formatting
        </button>
      </PopoverContent>
    </Popover>
  )
}
