import { Check } from "lucide-react";

export function TaskCheckbox({
  checked,
  color,
  label,
  onToggle,
}: {
  checked: boolean;
  color: string;
  label: string;
  onToggle: (next: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      aria-label={label}
      onPointerDown={(e) => e.stopPropagation()}
      onClick={(e) => {
        e.stopPropagation();
        onToggle(!checked);
      }}
      className="relative size-[14px] shrink-0 rounded-full border-2"
      style={{
        borderColor: color,
        background: checked ? color : "transparent",
      }}
    >
      {checked && (
        <Check
          className="absolute inset-0 m-auto size-2.5"
          strokeWidth={3}
          style={{ color: "hsl(var(--background))" }}
          aria-hidden
        />
      )}
    </button>
  );
}
