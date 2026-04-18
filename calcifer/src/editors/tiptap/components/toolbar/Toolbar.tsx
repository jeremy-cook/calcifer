import { useState, useEffect, useRef, Fragment } from 'react'
import { useEditorState, type Editor } from '@tiptap/react'
import { Toggle } from '~/components/ui/toggle'
import { Separator } from '~/components/ui/separator'
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
  TextSubscriptIcon,
  TextSuperscriptIcon,
  EraserIcon,
  ListBulletsIcon,
  ListNumbersIcon,
  ListChecksIcon,
  LinkIcon,
  CheckIcon,
} from '@phosphor-icons/react'
import { ColorPickerPopover } from './ColorPickerPopover'
import { FONT_FAMILIES, BLOCK_TYPES, ALIGNMENTS, type Alignment } from '../formattingOptions'
import { InsertPopover, type InsertSection } from './InsertPopover'

export type ToolbarSize = 'sm' | 'md' | 'lg'

type ToolbarGroup = {
  id: string
  toolbar: React.ReactNode
  insert: InsertSection
}

// Groups visible at each size; anything absent collapses into the Insert popover.
const VISIBLE: Record<ToolbarSize, ReadonlySet<string>> = {
  lg: new Set(['eraser', 'alignment', 'lists', 'subscript', 'colors', 'fontFamily', 'inline']),
  md: new Set(['colors', 'fontFamily', 'inline']),
  sm: new Set(['inline']),
}

// Breakpoints checked in descending order; falls back to 'sm'.
const BREAKPOINTS: [ToolbarSize, number][] = [
  ['lg', 1000],
  ['md', 630],
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
        const linkMark = state.schema.marks.link.create({ href: trimmedUrl, target: '_blank', rel: 'noopener noreferrer' })
        tr.replaceWith(from, to, state.schema.text(trimmedLabel, [linkMark]))
        return true
      })
      .run()
    setOpen(false)
  }

  return (
    <Popover open={open} onOpenChange={(o) => { if (!o) setOpen(false) }}>
      <PopoverAnchor asChild>
        <Toggle size="default" pressed={isLink} onClick={handleToggle} aria-label="Link">
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
        fontFamily: (ctx.editor.getAttributes('textStyle').fontFamily as string) ?? '',
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
    fontFamily,
    blockType,
    alignment,
  } = state
  const disableInline = isCode || isCodeBlock

  // Each group defines both its toolbar rendering and its overflow representation.
  // Array order = toolbar order (L→R) and overflow menu order.
  // Collapse behaviour is controlled entirely by VISIBLE — not by array position.
  const groups: ToolbarGroup[] = [
    {
      id: 'fontFamily',
      toolbar: (
        <Select
          value={fontFamily}
          onValueChange={(v) => {
            if (v === '__default__') editor.chain().focus().unsetFontFamily().run()
            else editor.chain().focus().setFontFamily(v).run()
          }}
          disabled={disableInline}
        >
          <SelectTrigger className="h-8 w-28 text-xs" aria-label="Font family">
            <SelectValue placeholder="Font" />
          </SelectTrigger>
          <SelectContent>
            {FONT_FAMILIES.map((f) => (
              <SelectItem
                key={f.value || '__default__'}
                value={f.value || '__default__'}
                style={{ fontFamily: f.value || undefined }}
              >
                {f.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      ),
      insert: {
        id: 'fontFamily',
        items: FONT_FAMILIES.map((f) => ({
          key: f.value || '__default__',
          label: f.label,
          icon: null,
          active: fontFamily === f.value,
          fontFamily: f.value || undefined,
          onClick: () =>
            f.value === ''
              ? editor.chain().focus().unsetFontFamily().run()
              : editor.chain().focus().setFontFamily(f.value).run(),
        })),
      },
    },
    {
      id: 'inline',
      toolbar: (
        <>
          <Toggle
            size="default"
            pressed={isBold}
            disabled={disableInline}
            onPressedChange={() => editor.chain().focus().toggleBold().run()}
            aria-label="Bold"
          >
            <TextBIcon weight="bold" />
          </Toggle>
          <Toggle
            size="default"
            pressed={isItalic}
            disabled={disableInline}
            onPressedChange={() => editor.chain().focus().toggleItalic().run()}
            aria-label="Italic"
          >
            <TextItalicIcon />
          </Toggle>
          <Toggle
            size="default"
            pressed={isUnderline}
            disabled={disableInline}
            onPressedChange={() => editor.chain().focus().toggleUnderline().run()}
            aria-label="Underline"
          >
            <TextUnderlineIcon />
          </Toggle>
          <Toggle
            size="default"
            pressed={isStrike}
            disabled={disableInline}
            onPressedChange={() => editor.chain().focus().toggleStrike().run()}
            aria-label="Strikethrough"
          >
            <TextStrikethroughIcon />
          </Toggle>
          <Toggle
            size="default"
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
      toolbar: (
        <>
          <ColorPickerPopover
            color={fontColor}
            onChange={(c) => editor.chain().focus().setColor(c).run()}
            onClear={() => editor.chain().focus().unsetColor().run()}
            icon={<TextAaIcon />}
            label="Font color"
            disabled={disableInline}
          />
          <ColorPickerPopover
            color={highlight}
            onChange={(c) => editor.chain().focus().setHighlight({ color: c }).run()}
            onClear={() => editor.chain().focus().unsetHighlight().run()}
            icon={<HighlighterIcon />}
            label="Highlight color"
            disabled={disableInline}
          />
        </>
      ),
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
      id: 'subscript',
      toolbar: (
        <>
          <Toggle
            size="default"
            pressed={isSubscript}
            disabled={disableInline}
            onPressedChange={() => editor.chain().focus().toggleSubscript().run()}
            aria-label="Subscript"
          >
            <TextSubscriptIcon />
          </Toggle>
          <Toggle
            size="default"
            pressed={isSuperscript}
            disabled={disableInline}
            onPressedChange={() => editor.chain().focus().toggleSuperscript().run()}
            aria-label="Superscript"
          >
            <TextSuperscriptIcon />
          </Toggle>
        </>
      ),
      insert: {
        id: 'subscript',
        items: [
          {
            key: 'subscript',
            label: 'Subscript',
            icon: <TextSubscriptIcon />,
            active: isSubscript,
            disabled: disableInline,
            onClick: () => editor.chain().focus().toggleSubscript().run(),
          },
          {
            key: 'superscript',
            label: 'Superscript',
            icon: <TextSuperscriptIcon />,
            active: isSuperscript,
            disabled: disableInline,
            onClick: () => editor.chain().focus().toggleSuperscript().run(),
          },
        ],
      },
    },
    {
      id: 'alignment',
      toolbar: (
        <>
          {ALIGNMENTS.map(({ value, icon, label }) => (
            <Toggle
              key={value}
              size="default"
              pressed={alignment === value}
              onPressedChange={() => editor.chain().focus().setTextAlign(value).run()}
              aria-label={label}
            >
              {icon}
            </Toggle>
          ))}
        </>
      ),
      insert: {
        id: 'alignment',
        items: ALIGNMENTS.map(({ value, icon, label }) => ({
          key: value,
          label,
          icon,
          active: alignment === value,
          onClick: () => editor.chain().focus().setTextAlign(value).run(),
        })),
      },
    },
    {
      id: 'lists',
      toolbar: (
        <>
          <Toggle
            size="default"
            pressed={isBulletList}
            onPressedChange={() => editor.chain().focus().toggleBulletList().run()}
            aria-label="Bullet list"
          >
            <ListBulletsIcon />
          </Toggle>
          <Toggle
            size="default"
            pressed={isOrderedList}
            onPressedChange={() => editor.chain().focus().toggleOrderedList().run()}
            aria-label="Numbered list"
          >
            <ListNumbersIcon />
          </Toggle>
          <Toggle
            size="default"
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
    {
      id: 'eraser',
      toolbar: (
        <Button
          variant="ghost"
          size="sm"
          className="h-8 min-w-8 px-1.5"
          onClick={() => editor.chain().focus().unsetAllMarks().run()}
          aria-label="Clear formatting"
        >
          <EraserIcon />
        </Button>
      ),
      insert: {
        id: 'eraser',
        items: [
          {
            key: 'eraser',
            label: 'Clear formatting',
            icon: <EraserIcon />,
            onClick: () => editor.chain().focus().unsetAllMarks().run(),
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
      className="flex shrink-0 items-center gap-0.5 overflow-hidden border-b border-border px-2 py-1"
    >
      {/* Block type — always visible */}
      <Select value={blockType} onValueChange={(v) => setBlockType(editor, v)}>
        <SelectTrigger className="h-8 w-32 text-xs" aria-label="Block type">
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
          <Separator orientation="vertical" className="mx-1 self-stretch" />
          {toolbar}
        </Fragment>
      ))}

      {/* Link — always visible */}
      <Separator orientation="vertical" className="mx-1 self-stretch" />
      <LinkPopover editor={editor} isLink={isLink} />

      <Separator orientation="vertical" className="mx-1 self-stretch" />

      {/* Insert — always visible, receives overflow items */}
      <InsertPopover editor={editor} sections={insertSections} />
    </div>
  )
}
