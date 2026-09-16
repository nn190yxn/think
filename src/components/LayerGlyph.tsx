import type { LayerGlyph as GlyphKind } from "../domain/layers";

/**
 * 六层的几何标记。颜色之外再给一个形状，保证不依赖颜色也能区分层次。
 */
export function LayerGlyph({
  glyph,
  size = 12,
}: {
  readonly glyph: GlyphKind;
  readonly size?: number;
}) {
  const common = {
    width: size,
    height: size,
    viewBox: "0 0 12 12",
    "aria-hidden": true,
    focusable: false,
  } as const;

  switch (glyph) {
    case "dot":
      return (
        <svg {...common}>
          <circle cx="6" cy="6" r="3.2" fill="currentColor" />
        </svg>
      );
    case "square":
      return (
        <svg {...common}>
          <rect x="2.6" y="2.6" width="6.8" height="6.8" fill="currentColor" />
        </svg>
      );
    case "triangle":
      return (
        <svg {...common}>
          <polygon points="6,2 10.4,9.6 1.6,9.6" fill="currentColor" />
        </svg>
      );
    case "wave":
      return (
        <svg {...common}>
          <path
            d="M1 4.4c1.6-2 3.4-2 5 0s3.4 2 5 0M1 8c1.6-2 3.4-2 5 0s3.4 2 5 0"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
          />
        </svg>
      );
    case "cross":
      return (
        <svg {...common}>
          <path
            d="M6 1.8v8.4M1.8 6h8.4"
            stroke="currentColor"
            strokeWidth="1.7"
            strokeLinecap="round"
          />
        </svg>
      );
    case "arrow":
      return (
        <svg {...common}>
          <path
            d="M2.2 3.4 6 7.2l3.8-3.8M2.2 7.6 6 11.4l3.8-3.8"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      );
  }
}
