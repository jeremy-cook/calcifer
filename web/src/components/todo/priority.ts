import { CellSignalHighIcon, CellSignalLowIcon, CellSignalMediumIcon, type Icon } from '@phosphor-icons/react'

// Presentation only: a priority option without an entry (e.g. newly added on the
// server) gets no icon and the plain text colour.
export const PRIORITY_ICONS: Record<string, Icon> = {
  low: CellSignalLowIcon,
  medium: CellSignalMediumIcon,
  high: CellSignalHighIcon,
}

export const PRIORITY_CLASSES: Record<string, string> = {
  low: 'text-muted-foreground',
  medium: 'text-chart-2',
  high: 'text-destructive',
}
