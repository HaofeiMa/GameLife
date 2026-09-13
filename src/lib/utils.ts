import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Merge conditional class names, with later Tailwind utilities winning
 *  over earlier ones of the same property. */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
