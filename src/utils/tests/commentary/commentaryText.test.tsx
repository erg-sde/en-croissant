import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";
import CommentaryText from "@/components/common/CommentaryText";
import type { ValidatedNarration } from "@/utils/commentary/types";
import { buildMoveCommentary } from "@/utils/commentary/report";
import { facts, run } from "./helpers";

function render(narration: ValidatedNarration, clickable = false): HTMLElement {
  const container = document.createElement("div");
  container.innerHTML = renderToStaticMarkup(
    <MantineProvider>
      <CommentaryText narration={narration} onMove={clickable ? () => {} : undefined} />
    </MantineProvider>,
  );
  return container;
}

const narration = buildMoveCommentary(facts("mateAllowed"), "??", run()).commentary!.narration;

test("declared moves become chips, with their move numbers", () => {
  const root = render(narration, true);
  const chips = Array.from(root.querySelectorAll("button")).map((b) => b.textContent);
  expect(chips).toContain("Nf6");
  expect(chips).toContain("4.Qxf7#");
  expect(root.textContent).toContain("Nf6 allows 4.Qxf7#, and White mates in 1.");
});

test("text is never parsed as markup", () => {
  const sentence = narration.sentences[0];
  const hostile = {
    sentences: [{ ...sentence, text: `${sentence.text} <img src=x onerror=alert(1)> **bold**` }],
  } as unknown as ValidatedNarration;
  const root = render(hostile);
  expect(root.querySelector("img, strong")).toBeNull();
  expect(root.textContent).toContain("<img src=x onerror=alert(1)> **bold**");
});
