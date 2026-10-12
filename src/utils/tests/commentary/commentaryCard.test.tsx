import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";
import CommentaryCard from "@/components/panels/analysis/CommentaryCard";
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
