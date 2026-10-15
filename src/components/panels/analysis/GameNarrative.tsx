import { Group, List, Paper, Stack, Text, UnstyledButton } from "@mantine/core";
import { memo } from "react";
import { useTranslation } from "react-i18next";
import CommentaryText from "@/components/common/CommentaryText";
import { ANNOTATION_INFO, isBasicAnnotation } from "@/utils/annotation";
import { moveNumber } from "@/utils/commentary/gameFacts";
import type { GameNarrative as GameNarrativeRecord } from "@/utils/commentary/types";
import { restoreCommentary } from "@/utils/commentary/validate";
import type { TreeNode } from "@/utils/treeReducer";

/** The mainline node at position index `index` (0 is the root). */
function mainlineNode(root: TreeNode, index: number): TreeNode | null {
  let node: TreeNode | undefined = root;
  for (let i = 0; i < index && node; i++) {
    node = node.children[0];
  }
  return node ?? null;
}

/**
 * The game summary in the report panel: the narrative, the takeaways, and the key moments (the
 * biggest swings and the decisive move), each a link to its move.
 */
function GameNarrative({
  narrative,
  root,
  onGoToMove,
}: {
  narrative: GameNarrativeRecord;
  root: TreeNode;
  onGoToMove: (path: number[]) => void;
}) {
  const { t } = useTranslation();
  const facts = narrative.facts;
  const keyIndices = [
    ...new Set([
      ...facts.swings.map((s) => s.index),
      ...(facts.decisive !== null ? [facts.decisive] : []),
    ]),
  ].sort((a, b) => a - b);

  return (
    <Paper withBorder p="md">
      <Stack gap="xs">
        <Text fw={600}>{t("Commentary.Narrative.Title")}</Text>
        <CommentaryText narration={narrative.narration} />

        {narrative.takeaways.length > 0 && (
          <>
            <Text size="sm" fw={500}>
              {t("Commentary.Narrative.Takeaways")}
            </Text>
            <List size="sm" spacing={2}>
              {narrative.takeaways.map((takeaway, i) => (
                <List.Item key={i}>
                  <CommentaryText narration={takeaway.narration} />
                </List.Item>
              ))}
            </List>
          </>
        )}

        {keyIndices.length > 0 && (
          <>
            <Text size="sm" fw={500}>
              {t("Commentary.Narrative.KeyMoments")}
            </Text>
            <Stack gap={4}>
              {keyIndices.map((index) => {
                const node = mainlineNode(root, index);
                if (!node?.san) return null;
                const glyph = node.annotations.find(isBasicAnnotation);
                const number = moveNumber(index, root.halfMoves);
                const label = `${number}${node.halfMoves % 2 === 1 ? "." : "..."}${node.san}${glyph ?? ""}`;
                const first = restoreCommentary(node.commentary)?.narration.sentences[0]?.text;
                return (
                  <UnstyledButton key={index} onClick={() => onGoToMove(Array(index).fill(0))}>
                    <Group gap="xs" wrap="nowrap" align="baseline">
                      <Text size="sm" fw={600} c={glyph ? ANNOTATION_INFO[glyph].color : undefined}>
                        {label}
                      </Text>
                      {first && (
                        <Text size="xs" c="dimmed" lineClamp={1}>
                          {first}
                        </Text>
                      )}
                    </Group>
                  </UnstyledButton>
                );
              })}
            </Stack>
          </>
        )}
      </Stack>
    </Paper>
  );
}

export default memo(GameNarrative);
