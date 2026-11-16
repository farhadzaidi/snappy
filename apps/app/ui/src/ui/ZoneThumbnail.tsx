import type { Zone } from "../core/model";

/** The gap around each zone, in pixels of the thumbnail. */
const INSET = 1.5;

/**
 * A small schematic of zones at a display's shape (`aspect`, width over height), with the
 * zones `highlighted` picks lit, and, when `numbered`, each zone's number.
 */
export function ZoneThumbnail({
  zones,
  aspect,
  height,
  highlighted,
  numbered = false,
}: {
  zones: Zone[];
  aspect: number;
  height: number;
  highlighted: (zone: Zone) => boolean;
  numbered?: boolean;
}) {
  const width = Math.round(height * aspect);
  return (
    <svg className="thumbnail" width={width} height={height} aria-hidden="true">
      {zones.map((zone, index) => {
        const x = zone.rect.x * width + INSET;
        const y = zone.rect.y * height + INSET;
        const w = Math.max(1, zone.rect.width * width - 2 * INSET);
        const h = Math.max(1, zone.rect.height * height - 2 * INSET);
        return (
          <g key={zone.id} data-highlighted={highlighted(zone)}>
            <rect x={x} y={y} width={w} height={h} rx={3} />
            {numbered && (
              <text x={x + w / 2} y={y + h / 2} textAnchor="middle" dominantBaseline="central">
                {index + 1}
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}
