import { Text, UnstyledButton } from "@mantine/core";
import { Fragment, memo } from "react";
import type { MoveRef } from "@/bindings";
import type { CommentarySentence, ValidatedNarration } from "@/utils/commentary/types";
import classes from "./CommentaryText.module.css";

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Splits a sentence into text and the moves it declares, with any move number in front. */
function segments(sentence: CommentarySentence): (string | { label: string; ref: MoveRef })[] {
  const moves = [...sentence.refs.moves].sort((a, b) => b.san.length - a.san.length);
  if (moves.length === 0) {
    return [sentence.text];
  }
  const pattern = new RegExp(
    `(?<![\\w])((?:\\d+\\.(?:\\.\\.)?)?(?:${moves.map((m) => escapeRegExp(m.san)).join("|")}))(?![\\w])`,
    "g",
  );
  const out: (string | { label: string; ref: MoveRef })[] = [];
  let last = 0;
  for (const match of sentence.text.matchAll(pattern)) {
    const san = match[1].replace(/^\d+\.(\.\.)?/, "");
    const ref = moves.find((m) => m.san === san);
    if (!ref || match.index === undefined) continue;
    out.push(sentence.text.slice(last, match.index), { label: match[1], ref });
    last = match.index + match[1].length;
  }
  out.push(sentence.text.slice(last));
  return out;
}

/**
 * Validated commentary as plain text. Moves a sentence declares are shown as chips; clicking one
 * calls `onMove` when given. Nothing is parsed as markup.
 */
function CommentaryText({
  narration,
  onMove,
  size = "sm",
}: {
  narration: ValidatedNarration;
  onMove?: (ref: MoveRef) => void;
  size?: string;
}) {
  return (
    <Text size={size} component="div">
      {narration.sentences.map((sentence, i) => (
        <Fragment key={i}>
          {i > 0 && " "}
          {segments(sentence).map((part, j) =>
            typeof part === "string" ? (
              <Fragment key={j}>{part}</Fragment>
            ) : onMove ? (
              <UnstyledButton key={j} className={classes.chip} onClick={() => onMove(part.ref)}>
                {part.label}
              </UnstyledButton>
            ) : (
              <span key={j} className={classes.chip}>
                {part.label}
              </span>
            ),
          )}
        </Fragment>
      ))}
    </Text>
  );
}

export default memo(CommentaryText);
