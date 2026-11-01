import type { Icon } from '@phosphor-icons/react'
import { PlusIcon } from '@phosphor-icons/react'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '~/components/ui/dropdown-menu'
import { PILL_CLASS, PILL_EMPTY_CLASS } from '~/components/entity/property-pill'
import { cn } from '~/lib/utils'
import type { SelectOption } from '~/model/structures'

export interface EntitySelectFieldProps {
  label: string
  value: string
  options: readonly SelectOption[]
  onChange: (value: string) => void
  // The empty pill's words, when a label shows above it already (a facet's band): "Set".
  emptyLabel?: string
  // The option that reads as unset (a priority of "None"): the pill shows "+ label".
  emptyValue?: string
  icon?: Icon
  // Extra classes for the pill, e.g. a colour for the current value.
  className?: string
  // An icon and colour per option key, shown in the menu (priority's signal bars).
  optionIcons?: Record<string, Icon>
  optionClasses?: Record<string, string>
}

// A pill showing the current option; it opens a menu of the others.
export function EntitySelectField({
  label,
  value,
  options,
  onChange,
  emptyValue,
  emptyLabel,
  icon: Icon,
  className,
  optionIcons,
  optionClasses,
}: EntitySelectFieldProps) {
  const current = options.find((option) => option.key === value)
  const empty = value === emptyValue || !current

  const renderEmptyTrigger = () => (
    <button type="button" className={PILL_EMPTY_CLASS} aria-label={`Set ${label.toLowerCase()}`}>
      <PlusIcon />
      {emptyLabel ?? label}
    </button>
  )

  const renderSetTrigger = () => (
    <button type="button" className={cn(PILL_CLASS, className)} aria-label={`${label}: ${current!.label}`}>
      {Icon && <Icon weight="fill" aria-hidden />}
      {current!.label}
    </button>
  )

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>{empty ? renderEmptyTrigger() : renderSetTrigger()}</DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-40">
        <DropdownMenuRadioGroup value={value} onValueChange={onChange}>
          {options.map((option) => {
            const OptionIcon = optionIcons?.[option.key]
            return (
              <DropdownMenuRadioItem key={option.key} value={option.key}>
                {OptionIcon && <OptionIcon weight="fill" className={optionClasses?.[option.key]} aria-hidden />}
                {option.label}
              </DropdownMenuRadioItem>
            )
          })}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
