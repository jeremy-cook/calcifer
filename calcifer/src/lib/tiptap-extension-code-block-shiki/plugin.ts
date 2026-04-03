import { findChildren } from '@tiptap/core'
import type { Node as ProsemirrorNode } from '@tiptap/pm/model'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import type { Step } from '@tiptap/pm/transform'
import { Decoration, DecorationSet } from '@tiptap/pm/view'
import { bundledLanguages } from 'shiki'
import type { Highlighter, BundledTheme, BundledLanguage } from 'shiki'

export const SHIKI_FORCE_DECORATION = 'shikiForceDecoration'

interface ShikiPluginOptions {
  name: string
  highlighter: Highlighter
  themes: { light: BundledTheme; dark: BundledTheme }
  defaultLanguage: string | null | undefined
}

interface GetDecorationsOptions extends ShikiPluginOptions {
  doc: ProsemirrorNode
}

function getDecorations({ doc, name, highlighter, themes, defaultLanguage }: GetDecorationsOptions): DecorationSet {
  const decorations: Decoration[] = []
  const languages = highlighter.getLoadedLanguages()

  findChildren(doc, (node) => node.type.name === name).forEach((block) => {
    let from = block.pos + 1
    const requestedLang = (block.node.attrs.language as string) || defaultLanguage || ''
    const isBundledLanguage = (l: string): l is BundledLanguage => l in bundledLanguages
    if (!isBundledLanguage(requestedLang) || !languages.includes(requestedLang)) return

    const lang = requestedLang
    const result = highlighter.codeToTokens(block.node.textContent, { lang, themes })

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
      // Advance past the newline between lines (not after the last line)
      if (lineIndex < result.tokens.length - 1) {
        from += 1
      }
    })
  })

  return DecorationSet.create(doc, decorations)
}

export function ShikiPlugin({ name, highlighter, themes, defaultLanguage }: ShikiPluginOptions) {
  const shikiPlugin: Plugin<DecorationSet> = new Plugin({
    key: new PluginKey('shiki'),

    state: {
      init: (_, { doc }) => getDecorations({ doc, name, highlighter, themes, defaultLanguage }),

      apply: (transaction, decorationSet, oldState, newState) => {
        if (transaction.getMeta(SHIKI_FORCE_DECORATION)) {
          return getDecorations({ doc: transaction.doc, name, highlighter, themes, defaultLanguage })
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
          return getDecorations({ doc: transaction.doc, name, highlighter, themes, defaultLanguage })
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
