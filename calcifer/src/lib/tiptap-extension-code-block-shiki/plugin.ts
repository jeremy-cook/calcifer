import { findChildren } from '@tiptap/core'
import type { Node as ProsemirrorNode } from '@tiptap/pm/model'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import type { Step } from '@tiptap/pm/transform'
import { Decoration, DecorationSet } from '@tiptap/pm/view'
import { createHighlighter, bundledLanguages } from 'shiki'
import type { Highlighter, BundledTheme, BundledLanguage } from 'shiki'

export const SHIKI_FORCE_DECORATION = 'shikiForceDecoration'

let highlighter: Highlighter | null = null
let highlighterPromise: Promise<void> | null = null

export type GetHighlighter = () => Highlighter

// May return null before async init completes
export function getDefaultHighlighter(): Highlighter | null {
  return highlighter
}

export function initHighlighter(themes: BundledTheme[], languages: BundledLanguage[]): Promise<void> {
  if (highlighter) return Promise.resolve()
  if (highlighterPromise) return highlighterPromise
  highlighterPromise = createHighlighter({ themes, langs: languages }).then((h) => {
    highlighter = h
  })
  return highlighterPromise
}

interface ShikiPluginOptions {
  name: string
  getHighlighter: GetHighlighter | null
  themes: { light: BundledTheme; dark: BundledTheme }
  defaultLanguage: string | null | undefined
}

interface GetDecorationsOptions extends ShikiPluginOptions {
  doc: ProsemirrorNode
}

const isBundledLanguage = (l: string): l is BundledLanguage => l in bundledLanguages

function getDecorations({ doc, name, getHighlighter: get, themes, defaultLanguage }: GetDecorationsOptions): DecorationSet {
  const h = get ? get() : getDefaultHighlighter()
  if (!h) return DecorationSet.empty

  const decorations: Decoration[] = []
  const languages = h.getLoadedLanguages()

  findChildren(doc, (node) => node.type.name === name).forEach((block) => {
    let from = block.pos + 1
    const requestedLang = (block.node.attrs.language as string) || defaultLanguage || ''
    if (!isBundledLanguage(requestedLang) || !languages.includes(requestedLang)) return

    const lang = requestedLang
    const result = h.codeToTokens(block.node.textContent, { lang, themes })

    const lightBg = result.bg
    const darkBg = h.getTheme(themes.dark)?.bg ?? lightBg
    decorations.push(
      Decoration.node(block.pos, block.pos + block.node.nodeSize, {
        class: 'shiki',
        style: `--shiki-light-bg:${lightBg};--shiki-dark-bg:${darkBg}`,
      }),
    )

    result.tokens.forEach((line, lineIndex) => {
      line.forEach((token) => {
        const to = from + token.content.length
        if (token.htmlStyle) {
          const style = Object.entries(token.htmlStyle)
            .map(([k, v]) => `${k}:${v}`)
            .join(';')
          decorations.push(Decoration.inline(from, to, { style }))
        }
        from = to
      })
      if (lineIndex < result.tokens.length - 1) {
        from += 1
      }
    })
  })

  return DecorationSet.create(doc, decorations)
}

export function ShikiPlugin({ name, getHighlighter: get, themes, defaultLanguage }: ShikiPluginOptions) {
  const shikiPlugin: Plugin<DecorationSet> = new Plugin({
    key: new PluginKey('shiki'),

    state: {
      init: (_, { doc }) => getDecorations({ doc, name, getHighlighter: get, themes, defaultLanguage }),

      apply: (transaction, decorationSet, oldState, newState) => {
        if (transaction.getMeta(SHIKI_FORCE_DECORATION)) {
          return getDecorations({ doc: transaction.doc, name, getHighlighter: get, themes, defaultLanguage })
        }

        const oldNodeName = oldState.selection.$head.parent.type.name
        const newNodeName = newState.selection.$head.parent.type.name
        const oldNodes = findChildren(oldState.doc, (node) => node.type.name === name)
        const newNodes = findChildren(newState.doc, (node) => node.type.name === name)

        const selectionInCodeBlock = [oldNodeName, newNodeName].includes(name)
        const codeBlockCountChanged = newNodes.length !== oldNodes.length
        const stepEncapsulatesCodeBlock = transaction.steps.some((step) => {
          const { from, to } = step as Step & { from?: number; to?: number }
          return (
            from !== undefined &&
            to !== undefined &&
            oldNodes.some((node) => node.pos >= from && node.pos + node.node.nodeSize <= to)
          )
        })

        if (transaction.docChanged && (selectionInCodeBlock || codeBlockCountChanged || stepEncapsulatesCodeBlock)) {
          return getDecorations({ doc: transaction.doc, name, getHighlighter: get, themes, defaultLanguage })
        }

        return decorationSet.map(transaction.mapping, transaction.doc)
      },
    },

    props: {
      decorations(state) {
        return shikiPlugin.getState(state)
      },
    },
  })

  return shikiPlugin
}
