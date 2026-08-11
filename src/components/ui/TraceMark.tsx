export function TraceMark({ compact = false }: { compact?: boolean }) {
  return (
    <svg
      className="trace-mark"
      width={compact ? 24 : 28}
      height={compact ? 24 : 28}
      viewBox="0 0 28 28"
      role="img"
      aria-label="MineTrace"
    >
      <path d="M3 5.5h22v5H3z" className="trace-mark__stone" />
      <path d="M3 11.5h8v5H3zM17 11.5h8v5h-8z" className="trace-mark__stone" />
      <path d="M11 11.5h6v5h-6z" className="trace-mark__seam" />
      <path d="M3 17.5h22v5H3z" className="trace-mark__stone" />
      <path d="M6 22.5h16v2H6z" className="trace-mark__trace" />
    </svg>
  );
}

