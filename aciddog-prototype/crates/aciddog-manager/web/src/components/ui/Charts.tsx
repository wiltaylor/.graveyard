import { For, Show, type JSX } from "solid-js";

export type DeltaTone = "up" | "down" | "flat";
export type SparkTone = "accent" | "positive" | "danger" | "warning";

export interface KpiTile {
  label: JSX.Element;
  value: JSX.Element;
  unit?: JSX.Element;
  meta?: JSX.Element;
  delta?: JSX.Element;
  deltaTone?: DeltaTone;
  spark?: number[];
  sparkTone?: SparkTone;
}

interface KpiRowProps {
  tiles: KpiTile[];
  columns?: number;
}

function buildSpark(values: number[], width = 240, height = 36): JSX.Element {
  if (values.length < 2) return null;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = Math.max(max - min, 0.0001);
  const step = width / (values.length - 1);
  let line = "";
  let area = `M0 ${height} `;
  values.forEach((v, i) => {
    const x = i * step;
    const y = height - ((v - min) / range) * height;
    line += `${i === 0 ? "M" : "L"}${x.toFixed(1)} ${y.toFixed(1)} `;
    area += `${i === 0 ? "L" : "L"}${x.toFixed(1)} ${y.toFixed(1)} `;
  });
  area += `L${width} ${height} Z`;
  return (
    <>
      <path class="spark-area" d={area} />
      <path class="spark-line" d={line} />
    </>
  );
}

export function KpiRow(props: KpiRowProps): JSX.Element {
  return (
    <div
      class="kpi-row"
      style={{
        "grid-template-columns": `repeat(${props.columns ?? props.tiles.length}, 1fr)`,
      }}
    >
      <For each={props.tiles}>
        {(tile) => (
          <div class="tile">
            <div class="tile-head">
              <span class="tile-label">{tile.label}</span>
              <Show when={tile.meta}>
                <span class="tile-meta">{tile.meta}</span>
              </Show>
            </div>
            <div class="tile-value">
              <span class="num">{tile.value}</span>
              <Show when={tile.unit}>
                <span class="unit">{tile.unit}</span>
              </Show>
              <Show when={tile.delta != null}>
                <span class={`tile-delta ${tile.deltaTone ?? "flat"}`}>
                  {tile.delta}
                </span>
              </Show>
            </div>
            <Show when={tile.spark && tile.spark.length > 1}>
              <svg
                class={`spark${
                  tile.sparkTone === "positive"
                    ? " positive"
                    : tile.sparkTone === "danger"
                      ? " danger"
                      : tile.sparkTone === "warning"
                        ? " warning"
                        : ""
                }`}
                viewBox="0 0 240 36"
                preserveAspectRatio="none"
                aria-hidden="true"
              >
                {buildSpark(tile.spark!)}
              </svg>
            </Show>
          </div>
        )}
      </For>
    </div>
  );
}

export default KpiRow;
