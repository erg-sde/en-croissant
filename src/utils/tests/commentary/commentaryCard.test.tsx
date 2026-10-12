import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";
import CommentaryCard from "@/components/panels/analysis/CommentaryCard";
import NarrationStatus from "@/components/panels/analysis/NarrationStatus";
import { buildMoveCommentary } from "@/utils/commentary/report";
import { facts, run } from "./helpers";

test("the card renders the kind, the badges, the text and the actions", () => {
  const f = facts("mateAllowed");
  const commentary = buildMoveCommentary(f, "??", run()).commentary!;
  const container = document.createElement("div");
  container.innerHTML = renderToStaticMarkup(
    <MantineProvider>
      <CommentaryCard
        commentary={commentary}
        facts={f}
        rootHalfMoves={0}
        onMove={() => {}}
        onAddToNotes={() => {}}
      />
    </MantineProvider>,
  );
  const text = container.textContent ?? "";
  // Without an i18n instance, labels render as their keys.
  expect(text).toContain("Commentary.Kind.mistake");
  expect(text).toContain("Commentary.Card.Perspective.both");
  expect(text).toContain("Commentary.Card.Offline");
  expect(text).toContain("Nf6 allows 4.Qxf7#");
  expect(text).toContain("Commentary.Card.BasedOn");
  expect(text).toContain("Commentary.Card.AddToNotes");
});

test("a model-written record shows the model that answered", () => {
  const commentary = buildMoveCommentary(facts("mateAllowed"), "??", run()).commentary!;
  const container = document.createElement("div");
  container.innerHTML = renderToStaticMarkup(
    <MantineProvider>
      <CommentaryCard
        commentary={{
          ...commentary,
          provenance: { ...commentary.provenance, source: "llm", model: "claude-opus-5-5" },
        }}
        facts={null}
        rootHalfMoves={0}
        onAddToNotes={() => {}}
      />
    </MantineProvider>,
  );
  const text = container.textContent ?? "";
  expect(text).toContain("claude-opus-5-5");
  expect(text).not.toContain("Commentary.Card.Offline");
});

test("narration status: running can be cancelled, a failure explains itself, a cancel is silent", () => {
  const render = (narration: Parameters<typeof NarrationStatus>[0]["narration"]) => {
    const container = document.createElement("div");
    container.innerHTML = renderToStaticMarkup(
      <MantineProvider>
        <NarrationStatus narration={narration} onCancel={() => {}} />
      </MantineProvider>,
    );
    // Mantine injects style tags; only rendered text counts.
    for (const style of container.querySelectorAll("style")) style.remove();
    return (container.textContent ?? "").trim();
  };
  expect(render({ status: "running" })).toContain("Common.Cancel");
  const failed = render({ status: "failed", failure: { kind: "auth", detail: { status: 401 } } });
  expect(failed).toContain("Errors.Commentary.Failed");
  expect(failed).toContain("Errors.Commentary.Auth");
  expect(render({ status: "cancelled" })).toBe("");
  expect(render(undefined)).toBe("");
});
