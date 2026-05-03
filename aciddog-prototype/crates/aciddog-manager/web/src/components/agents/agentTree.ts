// Pure helper that takes a flat list of `AgentRow` rows from the
// daemon and produces a grouped, parent->children tree shape ready
// for the Agents card to render. The rendering side is dumb on
// purpose — all of the bucketing, leader-pinning, hibernated-worker
// filtering, and orphan handling lives here so it can be unit-tested
// without spinning up Solid.
//
// Bucketing rules (mirrors the prior <Table> sort + the TUI sidebar's
// `(ungrouped)` placement):
//   - Buckets are discovered in FIRST-APPEARANCE order over the input.
//   - The single null/undefined bucket (rendered as "(ungrouped)" by
//     the renderer) ALWAYS sorts last, regardless of where the first
//     null-group row appears in the input.
//   - Within a bucket, agents whose `leader && parent === null` are
//     pinned to the top; everything else preserves first-appearance
//     order. Sort is stable.
//
// Worker handling:
//   - Workers (parent !== null) attach to their parent's children
//     IF the parent is in the SAME group bucket and is present in
//     the input.
//   - Workers with `status === "hibernated"` are filtered out unless
//     `opts.showHibernated === true` is passed. Top-level agents are
//     NEVER filtered by status — only workers.
//   - If a worker's parent does not resolve in the input, the worker
//     becomes a top-level orphan in its OWN group bucket (we trust
//     the worker's own `row.group`, never invent one from a missing
//     parent). Orphans can therefore mint a brand-new bucket purely
//     from their first appearance.
//
// Defensive:
//   - Any row with `archived === true` is dropped entirely (the
//     `AgentRow` schema does not yet expose `archived`, but the
//     daemon may grow it; we read defensively via a property check
//     so the helper is forward-compatible).

import type { AgentRow } from "../../types";

export interface AgentChild {
  row: AgentRow;
  /** True only when this worker's parent id was not present in the
   *  input rows. The renderer flags these with an "(orphaned)" tag. */
  orphaned?: boolean;
}

export interface AgentNode {
  row: AgentRow;
  children: AgentChild[];
}

export interface AgentGroup {
  /** `null` is the canonical sentinel for the ungrouped bucket; the
   *  renderer is responsible for printing "(ungrouped)" so this stays
   *  data-clean. */
  name: string | null;
  agents: AgentNode[];
  orphans: AgentChild[];
}

export interface GroupAgentsOptions {
  /** When true, hibernated workers stay in the tree. Default false
   *  matches the plan's "hide hibernated by default" requirement. */
  showHibernated?: boolean;
}

function isArchived(row: AgentRow): boolean {
  // `archived` is not declared on `AgentRow` (yet) — read defensively.
  const maybe = (row as unknown as { archived?: unknown }).archived;
  return maybe === true;
}

export function groupAgents(
  rows: AgentRow[],
  opts?: GroupAgentsOptions,
): AgentGroup[] {
  const showHibernated = opts?.showHibernated === true;

  // First pass: filter archived (always) + hibernated workers (default).
  const filtered: AgentRow[] = [];
  for (const row of rows) {
    if (isArchived(row)) continue;
    if (
      row.parent !== null &&
      row.status === "hibernated" &&
      !showHibernated
    ) {
      continue;
    }
    filtered.push(row);
  }

  // Build a lookup of top-level agents by id so we can decide which
  // workers attach vs. become orphans.
  const agentById = new Map<string, AgentRow>();
  for (const row of filtered) {
    if (row.parent === null) agentById.set(row.id, row);
  }

  // Bucket discovery pass: we walk the rows in first-appearance order
  // and mint a bucket the first time we see a new group key. Workers
  // contribute their OWN row.group when they orphan to a brand-new
  // bucket (mirrors the plan: "orphans attach to the group bucket via
  // the same first-appearance discovery").
  interface Bucket {
    name: string | null;
    /** Insertion-ordered list of top-level agent rows that belong to
     *  this bucket. We sort leader-first AT THE END so the first-
     *  appearance order is preserved within each tier. */
    agents: AgentRow[];
    /** Map from agent id -> children list. Lets us attach workers in
     *  O(1) without re-scanning the agents array. */
    children: Map<string, AgentChild[]>;
    orphans: AgentChild[];
  }

  const buckets = new Map<string | null, Bucket>();
  const order: (string | null)[] = [];

  const ensureBucket = (key: string | null): Bucket => {
    let b = buckets.get(key);
    if (!b) {
      b = { name: key, agents: [], children: new Map(), orphans: [] };
      buckets.set(key, b);
      order.push(key);
    }
    return b;
  };

  // Pass 2a: place top-level agents into buckets.
  for (const row of filtered) {
    if (row.parent !== null) continue;
    const key = row.group ?? null;
    const bucket = ensureBucket(key);
    bucket.agents.push(row);
    bucket.children.set(row.id, []);
  }

  // Pass 2b: place workers either under their parent (if the parent
  // exists in agentById AND is in the same group bucket) or as an
  // orphan in the worker's own group bucket.
  for (const row of filtered) {
    if (row.parent === null) continue;
    const parent = agentById.get(row.parent);
    const ownGroup = row.group ?? null;
    if (parent) {
      const parentGroup = parent.group ?? null;
      if (parentGroup === ownGroup) {
        // Same bucket — nest under the parent.
        const bucket = ensureBucket(parentGroup);
        const list = bucket.children.get(parent.id);
        // `list` will exist because pass 2a set it for every agent
        // in this bucket, but stay defensive.
        if (list) {
          list.push({ row });
          continue;
        }
      }
      // Parent exists but lives in a different bucket — treat the
      // worker as an orphan inside its OWN group bucket so the user
      // can still see the row. The plan only spells out "parent
      // doesn't resolve" -> orphan, so this cross-bucket case is a
      // defensive extension; flagging as `orphaned: true` gives the
      // user a visual cue.
      const bucket = ensureBucket(ownGroup);
      bucket.orphans.push({ row, orphaned: true });
      continue;
    }
    // Truly orphaned — parent string is not in the input.
    const bucket = ensureBucket(ownGroup);
    bucket.orphans.push({ row, orphaned: true });
  }

  // Pass 3: order the buckets — first-appearance order, with the null
  // bucket pinned LAST regardless of where it landed in the discovery
  // walk.
  const ordered: (string | null)[] = [];
  for (const key of order) {
    if (key !== null) ordered.push(key);
  }
  if (order.includes(null)) ordered.push(null);

  // Pass 4: leader-pin sort within each bucket (stable). We work on a
  // shallow copy so the bucket's first-appearance array isn't mutated
  // mid-iteration in the previous passes — they're already done, but
  // the explicit copy keeps the intent obvious.
  const result: AgentGroup[] = [];
  for (const key of ordered) {
    const bucket = buckets.get(key)!;
    const sortedAgents = [...bucket.agents].sort((a, b) => {
      const aLead = a.leader && a.parent === null;
      const bLead = b.leader && b.parent === null;
      if (aLead === bLead) return 0;
      return aLead ? -1 : 1;
    });
    const nodes: AgentNode[] = sortedAgents.map((row) => ({
      row,
      children: bucket.children.get(row.id) ?? [],
    }));
    result.push({
      name: bucket.name,
      agents: nodes,
      orphans: bucket.orphans,
    });
  }

  return result;
}
