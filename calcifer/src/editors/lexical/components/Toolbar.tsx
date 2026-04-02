import { useCallback, useEffect, useState } from 'react'
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext'
import { $getSelection, $isRangeSelection, $isTextNode, FORMAT_TEXT_COMMAND } from 'lexical'
import { $patchStyleText, $getSelectionStyleValueForProperty } from '@lexical/selection'
import { mergeRegister } from '@lexical/utils'
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
import { FONT_FAMILIES } from '~/editors/shared/formatting-options'

export function Toolbar() {
  const [editor] = useLexicalComposerContext()
  const [isBold, setIsBold] = useState(false)
  const [isItalic, setIsItalic] = useState(false)
  const [isUnderline, setIsUnderline] = useState(false)
  const [isStrikethrough, setIsStrikethrough] = useState(false)
  const [isCode, setIsCode] = useState(false)
  const [isSubscript, setIsSubscript] = useState(false)
  const [isSuperscript, setIsSuperscript] = useState(false)
  const [fontColor, setFontColor] = useState('')
  const [highlightColor, setHighlightColor] = useState('')
  const [fontFamily, setFontFamily] = useState('')

  const updateToolbar = useCallback(() => {
    const selection = $getSelection()
    if ($isRangeSelection(selection)) {
      setIsBold(selection.hasFormat('bold'))
      setIsItalic(selection.hasFormat('italic'))
      setIsUnderline(selection.hasFormat('underline'))
      setIsStrikethrough(selection.hasFormat('strikethrough'))
      setIsCode(selection.hasFormat('code'))
      setIsSubscript(selection.hasFormat('subscript'))
      setIsSuperscript(selection.hasFormat('superscript'))
      setFontColor($getSelectionStyleValueForProperty(selection, 'color', ''))
      setHighlightColor($getSelectionStyleValueForProperty(selection, 'background-color', ''))
      const rawFontFamily = $getSelectionStyleValueForProperty(selection, 'font-family', '')
      setFontFamily(rawFontFamily)
    }
  }, [])

  useEffect(() => {
    return mergeRegister(
      editor.registerUpdateListener(({ editorState }) => {
        editorState.read(() => {
          updateToolbar()
        })
      }),
    )
  }, [editor, updateToolbar])

  const applyStyle = useCallback(
    (styles: Record<string, string>) => {
      editor.update(() => {
        const selection = $getSelection()
        if ($isRangeSelection(selection)) {
          $patchStyleText(selection, styles)
        }
      })
    },
    [editor],
  )

  const clearFormatting = useCallback(() => {
    editor.update(() => {
      const selection = $getSelection()
      if (!$isRangeSelection(selection)) return
      $patchStyleText(selection, { color: '', 'background-color': '', 'font-family': '' })
      selection.getNodes().forEach((node) => {
        if ($isTextNode(node)) node.setFormat(0)
      })
    })
  }, [editor])

  return (
    <div className="flex shrink-0 flex-wrap items-center gap-0.5 border-b border-border px-2 py-1">
      {/* Font family */}
      <Select
        value={fontFamily}
        onValueChange={(v) => applyStyle({ 'font-family': v === '__default__' ? '' : v })}
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
      <Toggle size="sm" pressed={isBold} disabled={isCode} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'bold')} aria-label="Bold">
        <TextBIcon weight="bold" />
      </Toggle>
      <Toggle size="sm" pressed={isItalic} disabled={isCode} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'italic')} aria-label="Italic">
        <TextItalicIcon />
      </Toggle>
      <Toggle size="sm" pressed={isUnderline} disabled={isCode} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'underline')} aria-label="Underline">
        <TextUnderlineIcon />
      </Toggle>
      <Toggle size="sm" pressed={isStrikethrough} disabled={isCode} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'strikethrough')} aria-label="Strikethrough">
        <TextStrikethroughIcon />
      </Toggle>
      <Toggle size="sm" pressed={isCode} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'code')} aria-label="Inline code">
        <CodeIcon />
      </Toggle>

      <Separator orientation="vertical" className="mx-1 h-5" />

      {/* Color */}
      <ColorPickerPopover
        color={fontColor}
        onChange={(c) => applyStyle({ color: c })}
        onClear={() => applyStyle({ color: '' })}
        icon={<TextAaIcon />}
        label="Font color"
        disabled={isCode}
      />
      <ColorPickerPopover
        color={highlightColor}
        onChange={(c) => applyStyle({ 'background-color': c })}
        onClear={() => applyStyle({ 'background-color': '' })}
        icon={<HighlighterIcon />}
        label="Highlight color"
        disabled={isCode}
      />

      <Separator orientation="vertical" className="mx-1 h-5" />

      {/* Subscript / Superscript */}
      <Toggle size="sm" pressed={isSubscript} disabled={isCode} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'subscript')} aria-label="Subscript">
        <TextSubscriptIcon />
      </Toggle>
      <Toggle size="sm" pressed={isSuperscript} disabled={isCode} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'superscript')} aria-label="Superscript">
        <TextSuperscriptIcon />
      </Toggle>

      <Separator orientation="vertical" className="mx-1 h-5" />

      {/* Clear formatting */}
      <Button
        variant="ghost"
        size="sm"
        className="h-7 min-w-7 px-1.5"
        onClick={clearFormatting}
        aria-label="Clear formatting"
      >
        <EraserIcon />
      </Button>
    </div>
  )
}
