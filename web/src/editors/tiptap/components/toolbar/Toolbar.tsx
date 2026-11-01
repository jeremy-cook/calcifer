import { useState, useEffect, useRef, Fragment } from 'react'
import { useEditorState, type Editor } from '@tiptap/react'
import { Toggle } from '~/components/ui/toggle'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '~/components/ui/select'
import { Button } from '~/components/ui/button'
import { Popover, PopoverAnchor, PopoverContent } from '~/components/ui/popover'
import {
  TextBIcon,
  TextItalicIcon,
  TextUnderlineIcon,
  TextStrikethroughIcon,
  CodeIcon,
  TextAaIcon,
  HighlighterIcon,
  ListBulletsIcon,
  ListNumbersIcon,
  ListChecksIcon,
  LinkIcon,
  CheckIcon,
} from '@phosphor-icons/react'
import { BLOCK_TYPES, type Alignment } from '../formatting-options'
import { InsertPopover, type InsertSection } from './InsertPopover'
import { MorePopover } from './MorePopover'
import { TextColorPopover } from './TextColorPopover'

export type ToolbarSize = 'sm' | 'md' | 'lg'

type ToolbarGroup = {
  id: string
  toolbar: React.ReactNode
  insert: InsertSection
}

// Groups visible at each size; anything absent collapses into the Insert popover.
// Alignment, subscript and superscript, and clear formatting live in the More menu.
const VISIBLE: Record<ToolbarSize, ReadonlySet<string>> = {
  lg: new Set(['lists', 'colors', 'inline']),
  md: new Set(['colors', 'inline']),
  sm: new Set(['inline']),
}

// Breakpoints checked in descending order; falls back to 'sm'. Widths are inside
// the toolbar's padding.
const BREAKPOINTS: [ToolbarSize, number][] = [
  ['lg', 700],
  ['md', 540],
]

function getAutoSize(width: number): ToolbarSize {
  return BREAKPOINTS.find(([, min]) => width >= min)?.[0] ?? 'sm'
}

interface ToolbarProps {
  editor: Editor
  /** Pin a size (e.g. for a modal). Omit to auto-size from container width. */
  size?: ToolbarSize
}

function getBlockType(editor: Editor): string {
  if (editor.isActive('codeBlock')) return 'code'
  if (editor.isActive('heading', { level: 1 })) return 'h1'
  if (editor.isActive('heading', { level: 2 })) return 'h2'
  if (editor.isActive('heading', { level: 3 })) return 'h3'
  if (editor.isActive('blockquote')) return 'blockquote'
  return 'paragraph'
}

function setBlockType(editor: Editor, value: string) {
  switch (value) {
    case 'paragraph':
      editor.chain().focus().setParagraph().run()
      break
    case 'h1':
      editor.chain().focus().toggleHeading({ level: 1 }).run()
      break
    case 'h2':
      editor.chain().focus().toggleHeading({ level: 2 }).run()
      break
    case 'h3':
      editor.chain().focus().toggleHeading({ level: 3 }).run()
      break
    case 'blockquote':
      editor.chain().focus().toggleBlockquote().run()
      break
    case 'code':
      editor.chain().focus().toggleCodeBlock().run()
      break
  }
}

interface LinkPopoverProps {
  editor: Editor
  isLink: boolean
}

function LinkPopover({ editor, isLink }: LinkPopoverProps) {
  const [open, setOpen] = useState(false)
  const [label, setLabel] = useState('')
  const [url, setUrl] = useState('')

  const handleToggle = () => {
    if (isLink) {
      editor.chain().focus().unsetLink().run()
    } else {
      const { from, to } = editor.state.selection
      setLabel(editor.state.doc.textBetween(from, to))
      setUrl('')
      setOpen(true)
    }
  }

  const handleInsert = () => {
    const trimmedUrl = url.trim()
    if (!trimmedUrl) return
    const trimmedLabel = label.trim() || trimmedUrl
    editor
      .chain()
      .focus()
      .command(({ tr, state }) => {
        const { from, to } = state.selection
        const linkMark = state.schema.marks.link.create({
          href: trimmedUrl,
          target: '_blank',
          rel: 'noopener noreferrer',
        })
        tr.replaceWith(from, to, state.schema.text(trimmedLabel, [linkMark]))
        return true
      })
      .run()
    setOpen(false)
  }

  return (
    <Popover
      open={open}
      onOpenChange={(o) => {
        if (!o) setOpen(false)
      }}
    >
      <PopoverAnchor asChild>
        <Toggle variant="tool" pressed={isLink} onClick={handleToggle} aria-label="Link">
          <LinkIcon />
        </Toggle>
      </PopoverAnchor>
      <PopoverContent align="start" className="w-auto p-2">
        <div className="flex flex-col gap-1.5">
          <input
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleInsert()
              if (e.key === 'Escape') setOpen(false)
            }}
            className="w-64 rounded border border-border bg-background px-2 py-0.5 text-xs outline-none focus:border-primary"
            placeholder="Text"
          />
          <div className="flex items-center gap-1">
            <input
              value={url}
              onChange={(e) => {
                const val = e.target.value
                if (label === '' || label === url) setLabel(val)
                setUrl(val)
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleInsert()
                if (e.key === 'Escape') setOpen(false)
              }}
              className="w-56 rounded border border-border bg-background px-2 py-0.5 text-xs outline-none focus:border-primary"
              placeholder="https://"
              autoFocus
            />
            <Button variant="ghost" size="sm" className="h-6 w-6 p-0" onClick={handleInsert} aria-label="Apply link">
              <CheckIcon size={14} />
            </Button>
          </div>
        </div>
      </PopoverContent>
    </Popover>
  )
}

export function Toolbar({ editor, size: sizeProp }: ToolbarProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const [containerWidth, setContainerWidth] = useState(Infinity)

  useEffect(() => {
    if (sizeProp) return // no observer needed when size is fixed
    const el = containerRef.current
    if (!el) return
    const ro = new ResizeObserver(([entry]) => {
      setContainerWidth(entry.contentRect.width)
    })
    ro.observe(el)
    setContainerWidth(el.getBoundingClientRect().width)
    return () => ro.disconnect()
  }, [sizeProp])

  const show = VISIBLE[sizeProp ?? getAutoSize(containerWidth)]

  const state = useEditorState({
    editor,
    selector: (ctx) => {
      const blockType = getBlockType(ctx.editor)
      const attrNodeName = blockType.startsWith('h') ? 'heading' : blockType
      return {
        isBold: ctx.editor.isActive('bold'),
        isItalic: ctx.editor.isActive('italic'),
        isUnderline: ctx.editor.isActive('underline'),
        isStrike: ctx.editor.isActive('strike'),
        isCode: ctx.editor.isActive('code'),
        isCodeBlock: blockType === 'code',
        isSubscript: ctx.editor.isActive('subscript'),
        isSuperscript: ctx.editor.isActive('superscript'),
        isBulletList: ctx.editor.isActive('bulletList'),
        isOrderedList: ctx.editor.isActive('orderedList'),
        isTaskList: ctx.editor.isActive('taskList'),
        isLink: ctx.editor.isActive('link'),
        fontColor: (ctx.editor.getAttributes('textStyle').color as string) ?? '',
        highlight: (ctx.editor.getAttributes('highlight').color as string) ?? '',
        blockType,
        alignment: ((ctx.editor.getAttributes(attrNodeName).textAlign as string) || 'left') as Alignment,
      }
    },
  })

  const {
    isBold,
    isItalic,
    isUnderline,
    isStrike,
    isCode,
    isCodeBlock,
    isSubscript,
    isSuperscript,
    isBulletList,
    isOrderedList,
    isTaskList,
    isLink,
    fontColor,
    highlight,
    blockType,
    alignment,
  } = state
  const disableInline = isCode || isCodeBlock

  // Each group defines both its toolbar rendering and its overflow representation.
  // Array order = toolbar order (L→R) and overflow menu order.
  // Collapse behaviour is controlled entirely by VISIBLE — not by array position.
  const groups: ToolbarGroup[] = [
    {
      id: 'inline',
      toolbar: (
        <>
          <Toggle
            variant="tool"
            pressed={isBold}
            disabled={disableInline}
            onPressedChange={() => editor.chain().focus().toggleBold().run()}
            aria-label="Bold"
          >
            <TextBIcon weight="bold" />
          </Toggle>
          <Toggle
            variant="tool"
            pressed={isItalic}
            disabled={disableInline}
            onPressedChange={() => editor.chain().focus().toggleItalic().run()}
            aria-label="Italic"
          >
            <TextItalicIcon />
          </Toggle>
          <Toggle
            variant="tool"
            pressed={isUnderline}
            disabled={disableInline}
            onPressedChange={() => editor.chain().focus().toggleUnderline().run()}
            aria-label="Underline"
          >
            <TextUnderlineIcon />
          </Toggle>
          <Toggle
            variant="tool"
            pressed={isStrike}
            disabled={disableInline}
            onPressedChange={() => editor.chain().focus().toggleStrike().run()}
            aria-label="Strikethrough"
          >
            <TextStrikethroughIcon />
          </Toggle>
          <Toggle
            variant="tool"
            pressed={isCode}
            onPressedChange={() => editor.chain().focus().toggleCode().run()}
            aria-label="Inline code"
          >
            <CodeIcon />
          </Toggle>
        </>
      ),
      insert: {
        id: 'inline',
        items: [
          {
            key: 'bold',
            label: 'Bold',
            icon: <TextBIcon weight="bold" />,
            active: isBold,
            disabled: disableInline,
            onClick: () => editor.chain().focus().toggleBold().run(),
          },
          {
            key: 'italic',
            label: 'Italic',
            icon: <TextItalicIcon />,
            active: isItalic,
            disabled: disableInline,
            onClick: () => editor.chain().focus().toggleItalic().run(),
          },
          {
            key: 'underline',
            label: 'Underline',
            icon: <TextUnderlineIcon />,
            active: isUnderline,
            disabled: disableInline,
            onClick: () => editor.chain().focus().toggleUnderline().run(),
          },
          {
            key: 'strike',
            label: 'Strikethrough',
            icon: <TextStrikethroughIcon />,
            active: isStrike,
            disabled: disableInline,
            onClick: () => editor.chain().focus().toggleStrike().run(),
          },
          {
            key: 'code',
            label: 'Inline code',
            icon: <CodeIcon />,
            active: isCode,
            onClick: () => editor.chain().focus().toggleCode().run(),
          },
        ],
      },
    },
    {
      id: 'colors',
      toolbar: <TextColorPopover editor={editor} color={fontColor} highlight={highlight} disabled={disableInline} />,
      insert: {
        id: 'colors',
        items: [
          {
            key: 'fontColor',
            label: 'Font color',
            icon: <TextAaIcon />,
            disabled: disableInline,
            onClick: () => {},
            colorPicker: { value: fontColor, onChange: (c: string) => editor.chain().focus().setColor(c).run() },
          },
          {
            key: 'highlight',
            label: 'Highlight',
            icon: <HighlighterIcon />,
            disabled: disableInline,
            onClick: () => {},
            colorPicker: {
              value: highlight,
              onChange: (c: string) => editor.chain().focus().setHighlight({ color: c }).run(),
            },
          },
        ],
      },
    },
    {
      id: 'lists',
      toolbar: (
        <>
          <Toggle
            variant="tool"
            pressed={isBulletList}
            onPressedChange={() => editor.chain().focus().toggleBulletList().run()}
            aria-label="Bullet list"
          >
            <ListBulletsIcon />
          </Toggle>
          <Toggle
            variant="tool"
            pressed={isOrderedList}
            onPressedChange={() => editor.chain().focus().toggleOrderedList().run()}
            aria-label="Numbered list"
          >
            <ListNumbersIcon />
          </Toggle>
          <Toggle
            variant="tool"
            pressed={isTaskList}
            onPressedChange={() => editor.chain().focus().toggleTaskList().run()}
            aria-label="Checklist"
          >
            <ListChecksIcon />
          </Toggle>
        </>
      ),
      insert: {
        id: 'lists',
        items: [
          {
            key: 'bulletList',
            label: 'Bullet list',
            icon: <ListBulletsIcon />,
            active: isBulletList,
            onClick: () => editor.chain().focus().toggleBulletList().run(),
          },
          {
            key: 'orderedList',
            label: 'Numbered list',
            icon: <ListNumbersIcon />,
            active: isOrderedList,
            onClick: () => editor.chain().focus().toggleOrderedList().run(),
          },
          {
            key: 'taskList',
            label: 'Checklist',
            icon: <ListChecksIcon />,
            active: isTaskList,
            onClick: () => editor.chain().focus().toggleTaskList().run(),
          },
        ],
      },
    },
  ]

  const visibleGroups = groups.filter(({ id }) => show.has(id))
  const insertSections = groups.filter(({ id }) => !show.has(id)).map(({ insert }) => insert)

  return (
    <div
      ref={containerRef}
      className="flex shrink-0 items-center gap-0.5 overflow-hidden border-b border-border px-5 @3xl:px-14 py-1"
    >
      {/* Block type — always visible */}
      <Select value={blockType} onValueChange={(v) => setBlockType(editor, v)}>
        <SelectTrigger
          className="h-8 w-auto gap-2 border-0 bg-transparent px-2.5 text-sm font-semibold text-secondary-foreground shadow-none hover:bg-accent dark:bg-transparent dark:hover:bg-accent"
          aria-label="Block type"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {BLOCK_TYPES.map((b) => (
            <SelectItem key={b.value} value={b.value}>
              {b.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {/* Collapsible groups */}
      {visibleGroups.map(({ id, toolbar }) => (
        <Fragment key={id}>
          <ToolbarDivider />
          {toolbar}
        </Fragment>
      ))}

      {/* Link and More — always visible */}
      <ToolbarDivider />
      <LinkPopover editor={editor} isLink={isLink} />
      <MorePopover
        editor={editor}
        alignment={alignment}
        isSubscript={isSubscript}
        isSuperscript={isSuperscript}
        disableInline={disableInline}
      />

      {/* Insert — always visible, on the right, receives overflow items */}
      <InsertPopover editor={editor} sections={insertSections} />
    </div>
  )
}

function ToolbarDivider() {
  return <span className="mx-1.5 h-4 w-px shrink-0 bg-border" aria-hidden />
}
