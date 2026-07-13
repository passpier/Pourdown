import * as React from "react"
import { ChevronDown } from "lucide-react"
import { cn } from "@/lib/utils"

export type SelectProps = Omit<React.SelectHTMLAttributes<HTMLSelectElement>, "size">;

/**
 * Thin styled wrapper over a native <select>. No Radix/headless-ui Select
 * exists in this project (see components/ui) — a native element keeps full
 * keyboard nav and OS-native option rendering for free, at the cost of
 * pixel-level styling control we don't need here.
 */
const Select = React.forwardRef<HTMLSelectElement, SelectProps>(
  ({ className, children, ...props }, ref) => {
    return (
      <div className="relative inline-flex">
        <select
          ref={ref}
          className={cn(
            "h-8 appearance-none rounded-md border border-input bg-background pl-2.5 pr-7 text-sm text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
            className
          )}
          {...props}
        >
          {children}
        </select>
        <ChevronDown className="pointer-events-none absolute right-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
      </div>
    )
  }
)
Select.displayName = "Select"

export { Select }
