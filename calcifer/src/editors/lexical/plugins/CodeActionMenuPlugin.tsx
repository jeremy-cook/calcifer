import { type RefObject, useCallback, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext'
import { $getNodeByKey, $getSelection, $isRangeSelection } from 'lexical'
import { $isCodeNode, CodeNode } from '@lexical/code'
import { $getNearestNodeOfType } from '@lexical/utils'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '~/components/ui/select'
import { Button } from '~/components/ui/button'
import { CopySimpleIcon, CheckIcon } from '@phosphor-icons/react'
import { CODE_LANGUAGES } from '~/editors/shared/formatting-options'

interface Props {
  scrollContainerRef: RefObject<HTMLElement | null>
}

export function CodeActionMenuPlugin({ scrollContainerRef }: Props) {
  const scrollContainer = scrollContainerRef.current
  const [editor] = useLexicalComposerContext()
  const [visible, setVisible] = useState(false)
  const [lang, setLang] = useState('plaintext')
  const [menuPos, setMenuPos] = useState({ top: 0, right: 0 })
  const [copied, setCopied] = useState(false)
  const codeNodeKeyRef = useRef<string | null>(null)

  const updatePosition = useCallback(() => {
    const key = codeNodeKeyRef.current
    if (!key) return
    const codeDom = editor.getElementByKey(key)
    if (!codeDom) return
    const rect = codeDom.getBoundingClientRect()
    setMenuPos({
      top: rect.top + 4,
      right: window.innerWidth - rect.right + 8,
    })
  }, [editor])

  useEffect(() => {
    const unregister = editor.registerUpdateListener(({ editorState }) => {
      editorState.read(() => {
        const sel = $getSelection()
        if (!$isRangeSelection(sel)) {
          setVisible(false)
          codeNodeKeyRef.current = null
          return
        }
        const node = sel.anchor.getNode()
        const codeNode = $getNearestNodeOfType<CodeNode>(node, CodeNode)
        if (!codeNode) {
          setVisible(false)
          codeNodeKeyRef.current = null
          return
        }
        codeNodeKeyRef.current = codeNode.getKey()
        setLang(codeNode.getLanguage() ?? 'plaintext')
        setVisible(true)
        updatePosition()
      })
    })

    return unregister
  }, [editor, updatePosition])

  useEffect(() => {
    if (!scrollContainer) return
    const handleScroll = () => updatePosition()
    scrollContainer.addEventListener('scroll', handleScroll, { passive: true })
    return () => scrollContainer.removeEventListener('scroll', handleScroll)
  }, [scrollContainer, updatePosition])

  const handleLanguageChange = useCallback(
    (newLang: string) => {
      const key = codeNodeKeyRef.current
      if (!key) return
      editor.update(() => {
        const codeNode = $getNodeByKey<CodeNode>(key)
        if ($isCodeNode(codeNode)) codeNode.setLanguage(newLang)
      })
      setLang(newLang)
    },
    [editor],
  )

  const handleCopy = useCallback(() => {
    const key = codeNodeKeyRef.current
    if (!key) return
    editor.getEditorState().read(() => {
      const codeNode = $getNodeByKey<CodeNode>(key)
      if ($isCodeNode(codeNode)) {
        navigator.clipboard.writeText(codeNode.getTextContent())
      }
    })
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }, [editor])

  if (!visible) return null

  return createPortal(
    <div
      style={{ position: 'fixed', top: menuPos.top, right: menuPos.right, zIndex: 100 }}
      className="flex items-center gap-1 rounded border border-border bg-background px-2 py-1 shadow-sm"
    >
      <Select value={lang} onValueChange={handleLanguageChange}>
        <SelectTrigger className="h-6 w-32 border-0 bg-transparent px-1 text-xs shadow-none focus:ring-0">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {CODE_LANGUAGES.map((l) => (
            <SelectItem key={l.value} value={l.value} className="text-xs">
              {l.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={handleCopy} aria-label="Copy code">
        {copied ? <CheckIcon className="text-green-500" /> : <CopySimpleIcon />}
      </Button>
    </div>,
    document.body,
  )
}
