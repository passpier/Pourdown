import { type ClassValue, clsx } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function debounce<A extends unknown[]>(
  func: (...args: A) => unknown,
  wait: number
): (...args: A) => void {
  let timeout: ReturnType<typeof setTimeout> | null = null

  return function executedFunction(...args: A) {
    const later = () => {
      timeout = null
      func(...args)
    }

    if (timeout !== null) {
      clearTimeout(timeout)
    }
    timeout = setTimeout(later, wait)
  }
}
