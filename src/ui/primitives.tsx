import { useId, useState, type ReactNode } from "react";

const AVATAR_SIZES = { sm: 32, md: 40, lg: 56 } as const;

export function Avatar({
  src,
  alt,
  fallback,
  size = "md",
  className,
  "aria-label": ariaLabel,
}: {
  src?: string;
  alt?: string;
  fallback?: ReactNode;
  size?: keyof typeof AVATAR_SIZES | number;
  className?: string;
  "aria-label"?: string;
}) {
  const pixels = typeof size === "number" ? size : AVATAR_SIZES[size];
  const [error, setError] = useState(false);
  return (
    <span
      className={`avatar ${className ?? ""}`}
      style={{ inlineSize: pixels, blockSize: pixels }}
      role="img"
      aria-label={ariaLabel ?? alt}
    >
      {src && !error ? (
        <img
          src={src}
          alt={alt ?? ""}
          loading="lazy"
          className="avatar-image"
          onError={() => setError(true)}
        />
      ) : (
        <span className="avatar-fallback" aria-hidden="true">
          {fallback}
        </span>
      )}
    </span>
  );
}

/** 键帽组件。 */
export function Kbd({ children, className }: { children: ReactNode; className?: string }) {
  return <kbd className={`kbd ${className ?? ""}`}>{children}</kbd>;
}

/**
 * 分页组件。
 */
export function Pagination({
  page = 1,
  total = 1,
  onChange,
  className,
}: {
  page?: number;
  total?: number;
  onChange?: (page: number) => void;
  className?: string;
}) {
  const id = useId();
  if (total <= 1) return null;
  return (
    <nav className={`pagination ${className ?? ""}`} aria-label="分页">
      <button
        type="button"
        className="pagination-button"
        onClick={() => onChange?.(Math.max(1, page - 1))}
        disabled={page <= 1}
        aria-label="上一页"
      >
        ‹
      </button>
      <span className="pagination-status" aria-live="polite" id={id}>
        {page} / {total}
      </span>
      <button
        type="button"
        className="pagination-button"
        onClick={() => onChange?.(Math.min(total, page + 1))}
        disabled={page >= total}
        aria-label="下一页"
      >
        ›
      </button>
    </nav>
  );
}

/**
 * 滚动阴影容器。
 */
export function ScrollShadow({
  children,
  className,
  maxHeight,
}: {
  children: ReactNode;
  className?: string;
  maxHeight?: number | string;
}) {
  return (
    <div
      className={`scroll-shadow-container ${className ?? ""}`}
      style={maxHeight ? { maxHeight } : undefined}
    >
      {children}
    </div>
  );
}
