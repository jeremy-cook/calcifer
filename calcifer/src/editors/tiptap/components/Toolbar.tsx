import { useEditorState, type Editor } from '@tiptap/react'
import { Toggle } from '~/components/ui/toggle'
import { Separator } from '~/components/ui/separator'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '~/components/ui/select'
import { Button } from '~/components/ui/button'
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
} from '@phosphor-icons/react'
import { ColorPickerPopover } from '~/editors/shared/ColorPickerPopover'
import { FONT_FAMILIES, BLOCK_TYPES, ALIGNMENTS, type Alignment } from '~/editors/shared/formatting-options'

interface ToolbarProps {
  editor: Editor
}

function getBlockType(editor: Editor): string {
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
  }
}

export function Toolbar({ editor }: ToolbarProps) {
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
        isSubscript: ctx.editor.isActive('subscript'),
        isSuperscript: ctx.editor.isActive('superscript'),
        fontColor: (ctx.editor.getAttributes('textStyle').color as string) ?? '',
        highlight: (ctx.editor.getAttributes('highlight').color as string) ?? '',
        fontFamily: (ctx.editor.getAttributes('textStyle').fontFamily as string) ?? '',
        blockType,
        alignment: ((ctx.editor.getAttributes(attrNodeName).textAlign as string) || 'left') as Alignment,
      }
    },
  })

  const { isBold, isItalic, isUnderline, isStrike, isCode, isSubscript, isSuperscript, fontColor, highlight, fontFamily, blockType, alignment } = state

  return (
    <div className="flex shrink-0 flex-wrap items-center gap-0.5 border-b border-border px-2 py-1">
      {/* Block type */}
      <Select value={blockType} onValueChange={(v) => setBlockType(editor, v)}>
        <SelectTrigger className="h-7 w-32 text-xs" aria-label="Block type">
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

      <Separator orientation="vertical" className="mx-1 h-5" />

      {/* Font family */}
      <Select
        value={fontFamily}
        onValueChange={(v) => {
          if (v === '__default__') editor.chain().focus().unsetFontFamily().run()
          else editor.chain().focus().setFontFamily(v).run()
        }}
        disabled={isCode}
      >
        <SelectTrigger className="h-7 w-28 text-xs" aria-label="Font family">
          <SelectValue placeholder="Font" />
        </SelectTrigger>
        <SelectContent>
          {FONT_FAMILIES.map((f) => (
            <SelectItem key={f.value || '__default__'} value={f.value || '__default__'} style={{ fontFamily: f.value || undefined }}>
              {f.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <Separator orientation="vertical" className="mx-1 h-5" />

      {/* Inline formatting */}
      <Toggle size="sm" pressed={isBold} disabled={isCode} onPressedChange={() => editor.chain().focus().toggleBold().run()} aria-label="Bold">
        <TextBIcon weight="bold" />
      </Toggle>
      <Toggle size="sm" pressed={isItalic} disabled={isCode} onPressedChange={() => editor.chain().focus().toggleItalic().run()} aria-label="Italic">
        <TextItalicIcon />
      </Toggle>
      <Toggle size="sm" pressed={isUnderline} disabled={isCode} onPressedChange={() => editor.chain().focus().toggleUnderline().run()} aria-label="Underline">
        <TextUnderlineIcon />
      </Toggle>
      <Toggle size="sm" pressed={isStrike} disabled={isCode} onPressedChange={() => editor.chain().focus().toggleStrike().run()} aria-label="Strikethrough">
        <TextStrikethroughIcon />
      </Toggle>
      <Toggle size="sm" pressed={isCode} onPressedChange={() => editor.chain().focus().toggleCode().run()} aria-label="Inline code">
        <CodeIcon />
      </Toggle>

      <Separator orientation="vertical" className="mx-1 h-5" />

      {/* Color */}
      <ColorPickerPopover
        color={fontColor}
        onChange={(c) => editor.chain().focus().setColor(c).run()}
        onClear={() => editor.chain().focus().unsetColor().run()}
        icon={<TextAaIcon />}
        label="Font color"
        disabled={isCode}
      />
      <ColorPickerPopover
        color={highlight}
        onChange={(c) => editor.chain().focus().setHighlight({ color: c }).run()}
        onClear={() => editor.chain().focus().unsetHighlight().run()}
        icon={<HighlighterIcon />}
        label="Highlight color"
        disabled={isCode}
      />

      <Separator orientation="vertical" className="mx-1 h-5" />

      {/* Subscript / Superscript */}
      <Toggle size="sm" pressed={isSubscript} disabled={isCode} onPressedChange={() => editor.chain().focus().toggleSubscript().run()} aria-label="Subscript">
        <TextSubscriptIcon />
      </Toggle>
      <Toggle size="sm" pressed={isSuperscript} disabled={isCode} onPressedChange={() => editor.chain().focus().toggleSuperscript().run()} aria-label="Superscript">
        <TextSuperscriptIcon />
      </Toggle>

      <Separator orientation="vertical" className="mx-1 h-5" />

      {/* Alignment */}
      {ALIGNMENTS.map(({ value, icon, label }) => (
        <Toggle
          key={value}
          size="sm"
          pressed={alignment === value}
          onPressedChange={() => editor.chain().focus().setTextAlign(value).run()}
          aria-label={label}
        >
          {icon}
        </Toggle>
      ))}

      <Separator orientation="vertical" className="mx-1 h-5" />

      {/* Clear formatting */}
      <Button
        variant="ghost"
        size="sm"
        className="h-7 min-w-7 px-1.5"
        onClick={() => editor.chain().focus().unsetAllMarks().run()}
        aria-label="Clear formatting"
      >
        <EraserIcon />
      </Button>
    </div>
  )
}
