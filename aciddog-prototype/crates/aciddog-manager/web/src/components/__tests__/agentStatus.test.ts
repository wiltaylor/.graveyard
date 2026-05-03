import { describe, expect, it } from "vitest";

import { agentStatusVisual } from "../agents/agentStatus";

describe("agentStatusVisual round palette (agent / worker / unknown kind)", () => {
  it("ready -> green circle, success tone", () => {
    expect(agentStatusVisual("agent", "ready")).toEqual({
      glyph: "🟢",
      tone: "success",
      label: "ready",
    });
  });

  it("working -> yellow circle, warning tone", () => {
    expect(agentStatusVisual("agent", "working")).toEqual({
      glyph: "🟡",
      tone: "warning",
      label: "working",
    });
  });

  it("error -> red circle, danger tone", () => {
    expect(agentStatusVisual("agent", "error")).toEqual({
      glyph: "🔴",
      tone: "danger",
      label: "error",
    });
  });

  it("hibernated -> zzz, neutral tone", () => {
    expect(agentStatusVisual("agent", "hibernated")).toEqual({
      glyph: "💤",
      tone: "neutral",
      label: "hibernated",
    });
  });

  it("asleep -> moon, info tone", () => {
    expect(agentStatusVisual("agent", "asleep")).toEqual({
      glyph: "🌙",
      tone: "info",
      label: "asleep",
    });
  });

  it("kind='worker' falls through to the round palette", () => {
    expect(agentStatusVisual("worker", "ready")).toEqual({
      glyph: "🟢",
      tone: "success",
      label: "ready",
    });
    expect(agentStatusVisual("worker", "working")).toEqual({
      glyph: "🟡",
      tone: "warning",
      label: "working",
    });
    expect(agentStatusVisual("worker", "hibernated")).toEqual({
      glyph: "💤",
      tone: "neutral",
      label: "hibernated",
    });
  });

  it("unknown non-automaton kind falls through to the round palette", () => {
    expect(agentStatusVisual("future-kind", "ready")).toEqual({
      glyph: "🟢",
      tone: "success",
      label: "ready",
    });
  });

  it("unknown agent status returns the neutral default with raw label", () => {
    expect(agentStatusVisual("agent", "spinning")).toEqual({
      glyph: "⚪",
      tone: "neutral",
      label: "spinning",
    });
    expect(agentStatusVisual("worker", "")).toEqual({
      glyph: "⚪",
      tone: "neutral",
      label: "",
    });
  });
});

describe("agentStatusVisual square palette (kind='automaton')", () => {
  it("ready -> green square, success tone", () => {
    expect(agentStatusVisual("automaton", "ready")).toEqual({
      glyph: "🟩",
      tone: "success",
      label: "ready",
    });
  });

  it("working -> yellow square, warning tone", () => {
    expect(agentStatusVisual("automaton", "working")).toEqual({
      glyph: "🟨",
      tone: "warning",
      label: "working",
    });
  });

  it("error -> red square, danger tone", () => {
    expect(agentStatusVisual("automaton", "error")).toEqual({
      glyph: "🟥",
      tone: "danger",
      label: "error",
    });
  });

  it("asleep -> blue square, info tone", () => {
    expect(agentStatusVisual("automaton", "asleep")).toEqual({
      glyph: "🟦",
      tone: "info",
      label: "asleep",
    });
  });

  it("unknown automaton status falls back to the ready glyph (TUI catch-all)", () => {
    expect(agentStatusVisual("automaton", "hibernated")).toEqual({
      glyph: "🟩",
      tone: "success",
      label: "ready",
    });
    expect(agentStatusVisual("automaton", "anything-else")).toEqual({
      glyph: "🟩",
      tone: "success",
      label: "ready",
    });
  });
});
