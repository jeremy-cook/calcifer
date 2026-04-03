import { useCallback, useEffect, useState } from 'react'
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext'
import {
  $getSelection,
  $isRangeSelection,
  $isElementNode,
  $isTextNode,
  FORMAT_TEXT_COMMAND,
  FORMAT_ELEMENT_COMMAND,
  $createParagraphNode,
  type ElementFormatType,
} from 'lexical'
import { $setBlocksType } from '@lexical/selection'
import { $patchStyleText, $getSelectionStyleValueForProperty } from '@lexical/selection'
import { mergeRegister } from '@lexical/utils'
import { $isHeadingNode, $isQuoteNode, $createHeadingNode, $createQuoteNode, type HeadingTagType } from '@lexical/rich-text'
import { $findMatchingParent } from '@lexical/utils'
import { $isRootOrShadowRoot } from 'lexical'
import { $isListNode, INSERT_ORDERED_LIST_COMMAND, INSERT_UNORDERED_LIST_COMMAND, INSERT_CHECK_LIST_COMMAND, REMOVE_LIST_COMMAND } from '@lexical/list'
import { $createCodeNode, $isCodeNode } from '@lexical/code'
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
  ListBulletsIcon,
  ListNumbersIcon,
  ListChecksIcon,
} from '@phosphor-icons/react'
import { ColorPickerPopover } from '~/editors/shared/ColorPickerPopover'
import { FONT_FAMILIES, BLOCK_TYPES, ALIGNMENTS, type Alignment } from '~/editors/shared/formatting-options'

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
  const [blockType, setBlockType] = useState('paragraph')
  const [alignment, setAlignment] = useState<Alignment>('left')

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
      setFontFamily($getSelectionStyleValueForProperty(selection, 'font-family', ''))

      const anchorNode = selection.anchor.getNode()
      const element =
        anchorNode.getKey() === 'root'
          ? anchorNode
          : $findMatchingParent(anchorNode, (e) => {
              const parent = e.getParent()
              return parent !== null && $isRootOrShadowRoot(parent)
            })

      if (element !== null) {
        if ($isListNode(element)) {
          setBlockType(element.getListType())
        } else if ($isHeadingNode(element)) {
          setBlockType(element.getTag())
        } else if ($isQuoteNode(element)) {
          setBlockType('blockquote')
        } else if ($isCodeNode(element)) {
          setBlockType('code')
        } else {
          setBlockType('paragraph')
        }
        if ($isElementNode(element)) {
          setAlignment((element.getFormatType() as Alignment) || 'left')
        }
      }
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

  const changeBlockType = useCallback(
    (value: string) => {
      if (value === 'bullet') {
        if (blockType === 'bullet') editor.dispatchCommand(REMOVE_LIST_COMMAND, undefined)
        else editor.dispatchCommand(INSERT_UNORDERED_LIST_COMMAND, undefined)
        return
      }
      if (value === 'number') {
        if (blockType === 'number') editor.dispatchCommand(REMOVE_LIST_COMMAND, undefined)
        else editor.dispatchCommand(INSERT_ORDERED_LIST_COMMAND, undefined)
        return
      }
      editor.update(() => {
        const selection = $getSelection()
        if (!$isRangeSelection(selection)) return
        if (value === 'paragraph') {
          $setBlocksType(selection, () => $createParagraphNode())
        } else if (value === 'blockquote') {
          $setBlocksType(selection, () => $createQuoteNode())
        } else if (value === 'h1' || value === 'h2' || value === 'h3') {
          $setBlocksType(selection, () => $createHeadingNode(value as HeadingTagType))
        } else if (value === 'code') {
          $setBlocksType(selection, () => $createCodeNode())
        }
      })
    },
    [editor, blockType],
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

  const toggleBulletList = useCallback(() => {
    if (blockType === 'bullet') {
      editor.dispatchCommand(REMOVE_LIST_COMMAND, undefined)
    } else {
      editor.dispatchCommand(INSERT_UNORDERED_LIST_COMMAND, undefined)
    }
  }, [editor, blockType])

  const toggleOrderedList = useCallback(() => {
    if (blockType === 'number') {
      editor.dispatchCommand(REMOVE_LIST_COMMAND, undefined)
    } else {
      editor.dispatchCommand(INSERT_ORDERED_LIST_COMMAND, undefined)
    }
  }, [editor, blockType])

  const toggleCheckList = useCallback(() => {
    if (blockType === 'check') {
      editor.dispatchCommand(REMOVE_LIST_COMMAND, undefined)
    } else {
      editor.dispatchCommand(INSERT_CHECK_LIST_COMMAND, undefined)
    }
  }, [editor, blockType])

  const isCodeBlock = blockType === 'code'
  const disableInline = isCode || isCodeBlock

  return (
    <div className="flex shrink-0 flex-wrap items-center gap-0.5 border-b border-border px-2 py-1">
      {/* Block type */}
      <Select value={blockType} onValueChange={changeBlockType}>
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
        onValueChange={(v) => applyStyle({ 'font-family': v === '__default__' ? '' : v })}
        disabled={disableInline}
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
      <Toggle size="sm" pressed={isBold} disabled={disableInline} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'bold')} aria-label="Bold">
        <TextBIcon weight="bold" />
      </Toggle>
      <Toggle size="sm" pressed={isItalic} disabled={disableInline} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'italic')} aria-label="Italic">
        <TextItalicIcon />
      </Toggle>
      <Toggle size="sm" pressed={isUnderline} disabled={disableInline} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'underline')} aria-label="Underline">
        <TextUnderlineIcon />
      </Toggle>
      <Toggle size="sm" pressed={isStrikethrough} disabled={disableInline} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'strikethrough')} aria-label="Strikethrough">
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
        disabled={disableInline}
      />
      <ColorPickerPopover
        color={highlightColor}
        onChange={(c) => applyStyle({ 'background-color': c })}
        onClear={() => applyStyle({ 'background-color': '' })}
        icon={<HighlighterIcon />}
        label="Highlight color"
        disabled={disableInline}
      />

      <Separator orientation="vertical" className="mx-1 h-5" />

      {/* Subscript / Superscript */}
      <Toggle size="sm" pressed={isSubscript} disabled={disableInline} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'subscript')} aria-label="Subscript">
        <TextSubscriptIcon />
      </Toggle>
      <Toggle size="sm" pressed={isSuperscript} disabled={disableInline} onPressedChange={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'superscript')} aria-label="Superscript">
        <TextSuperscriptIcon />
      </Toggle>

      <Separator orientation="vertical" className="mx-1 h-5" />

      {/* Alignment */}
      {ALIGNMENTS.map(({ value, icon, label }) => (
        <Toggle
          key={value}
          size="sm"
          pressed={alignment === value}
          onPressedChange={() => editor.dispatchCommand(FORMAT_ELEMENT_COMMAND, value as ElementFormatType)}
          aria-label={label}
        >
          {icon}
        </Toggle>
      ))}

      <Separator orientation="vertical" className="mx-1 h-5" />

      {/* Lists */}
      <Toggle size="sm" pressed={blockType === 'bullet'} onPressedChange={toggleBulletList} aria-label="Bullet list">
        <ListBulletsIcon />
      </Toggle>
      <Toggle size="sm" pressed={blockType === 'number'} onPressedChange={toggleOrderedList} aria-label="Numbered list">
        <ListNumbersIcon />
      </Toggle>
      <Toggle size="sm" pressed={blockType === 'check'} onPressedChange={toggleCheckList} aria-label="Checklist">
        <ListChecksIcon />
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
