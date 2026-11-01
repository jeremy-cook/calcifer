// A property shown as a pill in an entity's header.
export const PILL_CLASS =
  'inline-flex h-7 items-center gap-1.5 rounded-full bg-muted px-2.5 text-sm font-semibold whitespace-nowrap text-secondary-foreground outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring [&_svg]:size-4 [&_svg]:shrink-0'

// An unset property or an action ("+ Tag"): no fill until hovered.
export const PILL_EMPTY_CLASS =
  'inline-flex h-7 items-center gap-1 rounded-full px-2 text-sm font-medium whitespace-nowrap text-muted-foreground outline-none hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring [&_svg]:size-3.5 [&_svg]:shrink-0'

// The × inside a pill that clears or removes it.
export const PILL_CLEAR_CLASS =
  '-mr-1 inline-flex size-5 items-center justify-center rounded-full text-muted-foreground hover:bg-background/60 hover:text-destructive [&_svg]:size-3!'
