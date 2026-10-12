import { Badge, Button, Collapse, Group, List, Paper, Stack, Text } from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import { IconNotes } from "@tabler/icons-react";
import { memo, useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { MoveFacts, MoveRef } from "@/bindings";
import CommentaryText from "@/components/common/CommentaryText";
import { enumerateFacts } from "@/utils/commentary/salience";
import { formatNumber } from "@/utils/commentary/segmentize";
import type { MoveCommentary } from "@/utils/commentary/types";

/**
 * The commentary of the current move: why it gets commentary, the text, and, while the report's
 * facts are in memory, every fact the text could have used.
 */
function CommentaryCard({
  commentary,
  facts,
  rootHalfMoves,
  onMove,
  onAddToNotes,
}: {
  commentary: MoveCommentary;
  facts: MoveFacts | null;
  rootHalfMoves: number;
  onMove?: (ref: MoveRef) => void;
  onAddToNotes: (text: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const [basisOpen, { toggle: toggleBasis }] = useDisclosure(false);
  const text = commentary.narration.sentences.map((s) => s.text).join(" ");

  const basis = useMemo(
    () =>
      facts
        ? enumerateFacts(facts, { rootHalfMoves }).map((fact) => {
            const v = fact.vocabulary;
            const detail = [
              v.moves.map((m) => m.san).join(" "),
              v.moves.length === 0 ? v.squares.join(", ") : "",
              v.numbers.map((n) => formatNumber(n, i18n.language)).join(", "),
            ]
              .filter(Boolean)
              .join(" · ");
            return { id: fact.id, label: t(`Commentary.FactKind.${fact.kind}`), detail };
          })
        : [],
    [facts, rootHalfMoves, t, i18n.language],
  );

  return (
    <Paper withBorder p="md">
      <Stack gap="xs">
        <Group justify="space-between" wrap="nowrap">
          <Text fw={600}>{t(`Commentary.Kind.${commentary.kind}`)}</Text>
          <Group gap={4} wrap="nowrap">
            <Badge variant="light" size="sm">
              {t(`Commentary.Card.Perspective.${commentary.perspective}`)}
            </Badge>
            {commentary.provenance.source === "deterministic" ? (
              <Badge variant="outline" size="sm">
                {t("Commentary.Card.Offline")}
              </Badge>
            ) : (
              commentary.provenance.model && (
                <Badge variant="outline" size="sm" tt="none">
                  {commentary.provenance.model}
                </Badge>
              )
            )}
          </Group>
        </Group>
        <CommentaryText narration={commentary.narration} onMove={onMove} />
        <Group justify="space-between">
          {basis.length > 0 ? (
            <Button variant="subtle" size="compact-xs" onClick={toggleBasis}>
              {t("Commentary.Card.BasedOn")}
            </Button>
          ) : (
            <span />
          )}
          <Button
            variant="subtle"
            size="compact-xs"
            leftSection={<IconNotes size="0.875rem" />}
            onClick={() => onAddToNotes(text)}
          >
            {t("Commentary.Card.AddToNotes")}
          </Button>
        </Group>
        <Collapse in={basisOpen}>
          <List size="xs" spacing={2}>
            {basis.map((item) => (
              <List.Item key={item.id}>
                <Text span size="xs" fw={500}>
                  {item.label}
                </Text>
                {item.detail && (
                  <Text span size="xs" c="dimmed">
                    {" "}
                    {item.detail}
                  </Text>
                )}
              </List.Item>
            ))}
          </List>
        </Collapse>
      </Stack>
    </Paper>
  );
}

export default memo(CommentaryCard);
