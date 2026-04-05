import {
  DecoratorNode,
  type LexicalNode,
  type NodeKey,
  type SerializedLexicalNode,
  type Spread,
  $applyNodeReplacement,
} from 'lexical'
import { Suspense, lazy } from 'react'

export type SerializedImageNode = Spread<
  { src: string; altText: string; width?: number | 'inherit'; caption?: string },
  SerializedLexicalNode
>

// Lazy-load the component to avoid circular dependency
const ImageComponent = lazy(() =>
  import('../components/ImageComponent').then((m) => ({ default: m.ImageComponent })),
)

export class ImageNode extends DecoratorNode<React.ReactNode> {
  __src: string
  __altText: string
  __width: number | 'inherit'
  __caption: string

  static getType(): string {
    return 'image'
  }

  static clone(node: ImageNode): ImageNode {
    return new ImageNode(node.__src, node.__altText, node.__width, node.__caption, node.__key)
  }

  constructor(
    src: string,
    altText: string,
    width: number | 'inherit' = 'inherit',
    caption = '',
    key?: NodeKey,
  ) {
    super(key)
    this.__src = src
    this.__altText = altText
    this.__width = width
    this.__caption = caption
  }

  static importJSON(serializedNode: SerializedImageNode): ImageNode {
    return $createImageNode({
      src: serializedNode.src,
      altText: serializedNode.altText,
      width: serializedNode.width,
      caption: serializedNode.caption,
    })
  }

  exportJSON(): SerializedImageNode {
    return {
      type: 'image',
      version: 1,
      src: this.__src,
      altText: this.__altText,
      width: this.__width,
      caption: this.__caption ?? '',
    }
  }

  createDOM(): HTMLElement {
    const div = document.createElement('div')
    div.style.display = 'contents'
    return div
  }

  updateDOM(): false {
    return false
  }

  getSrc(): string {
    return this.__src
  }

  getAltText(): string {
    return this.__altText
  }

  getWidth(): number | 'inherit' {
    return this.__width
  }

  getCaption(): string {
    return this.__caption
  }

  setWidth(width: number | 'inherit'): void {
    const writable = this.getWritable()
    writable.__width = width
  }

  setCaption(caption: string): void {
    const writable = this.getWritable()
    writable.__caption = caption
  }

  decorate(): React.ReactNode {
    return (
      <Suspense fallback={null}>
        <ImageComponent
          src={this.__src}
          altText={this.__altText}
          width={this.__width}
          caption={this.__caption}
          nodeKey={this.getKey()}
        />
      </Suspense>
    )
  }

  isInline(): false {
    return false
  }
}

export function $createImageNode({
  src,
  altText = '',
  width,
  caption,
}: {
  src: string
  altText?: string
  width?: number | 'inherit'
  caption?: string
}): ImageNode {
  return $applyNodeReplacement(new ImageNode(src, altText, width ?? 'inherit', caption ?? ''))
}

export function $isImageNode(node: LexicalNode | null | undefined): node is ImageNode {
  return node instanceof ImageNode
}
