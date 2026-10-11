import { Alert, Grid, Group, Paper, ScrollArea, Stack, Text } from "@mantine/core";
import { IconZoomCheck } from "@tabler/icons-react";
import cx from "clsx";
import equal from "fast-deep-equal";
import { useAtom, useAtomValue } from "jotai";
import React, { memo, useCallback, useContext, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useStore } from "zustand";
import { commands } from "@/bindings";
import EvalChart from "@/components/common/EvalChart";
import ProgressButton from "@/components/common/ProgressButton";
import { TreeStateContext } from "@/components/common/TreeStateContext";
import type { MoveRef } from "@/bindings";
import {
  activeTabAtom,
  commentaryFactsFamily,
  commentaryFactsKey,
  currentReportModalOpenAtom,
} from "@/state/atoms";
import { ANNOTATION_INFO, isBasicAnnotation } from "@/utils/annotation";
import { getGameStats, getVariationLine } from "@/utils/chess";
import { restoreCommentary } from "@/utils/commentary/validate";
import { getNodeAtPath } from "@/utils/treeReducer";
import classes from "./AnalysisPanel.module.css";
import CommentaryCard from "./CommentaryCard";
import ReportModal from "./ReportModal";

function ReportPanel() {
  const { t } = useTranslation();

  const activeTab = useAtomValue(activeTabAtom);

  const store = useContext(TreeStateContext)!;
  const root = useStore(store, (s) => s.root);
  const [reportingMode, setReportingMode] = useAtom(currentReportModalOpenAtom);

  const inProgress = useStore(store, (s) => s.report.inProgress);
  const setInProgress = useStore(store, (s) => s.setReportInProgress);
  const [reportError, setReportError] = useState<string | null>(null);

  const position = useStore(store, (s) => s.position);
  const goToMove = useStore(store, (s) => s.goToMove);
  const makeMoves = useStore(store, (s) => s.makeMoves);
  const appendComment = useStore(store, (s) => s.appendComment);
  const factsByMove = useAtomValue(commentaryFactsFamily(activeTab ?? ""));
  const currentNode = getNodeAtPath(root, position);
  const commentary = useMemo(
    () => restoreCommentary(currentNode.commentary),
    [currentNode.commentary],
  );
  const facts =
    factsByMove.get(commentaryFactsKey(root.fen, getVariationLine(root, position, true))) ?? null;

  // Plays a move the commentary cites, with the line leading to it, as a variation.
  const playCitedMove = useCallback(
    (ref: MoveRef) => {
      if (!facts || ref.at.t !== "line") return;
      const line = facts.lines[ref.at.id];
      const start = line.origin === "before" ? position.slice(0, -1) : position;
      goToMove(start);
      makeMoves({ payload: line.pvUci.slice(0, ref.at.ply + 1) });
    },
    [facts, position, goToMove, makeMoves],
  );

  const stats = useMemo(() => getGameStats(root), [root]);

  const handleCancel = useCallback(() => {
    commands.cancelAnalysis(`report_${activeTab}`);
  }, [activeTab]);

  const openReportingMode = useCallback(() => {
    setReportingMode(true);
  }, [setReportingMode]);

  const closeReportingMode = useCallback(() => {
    setReportingMode(false);
  }, [setReportingMode]);

  return (
    <ScrollArea offsetScrollbars>
      <ReportModal
        tab={activeTab!}
        reportingMode={reportingMode}
        closeReportingMode={closeReportingMode}
        onError={setReportError}
      />
      <Stack mb="lg" gap="0.4rem" mr="xs">
        <ProgressButton
          id={`report_${activeTab}`}
          redoable
          disabled={root.children.length === 0}
          leftIcon={<IconZoomCheck size="0.875rem" />}
          onClick={openReportingMode}
          onCancel={handleCancel}
          initInstalled={false}
          labels={{
            action: t("Board.Analysis.GenerateReport"),
            completed: t("Board.Analysis.ReportGenerated"),
            inProgress: t("Board.Analysis.GeneratingReport"),
          }}
          inProgress={inProgress}
          setInProgress={setInProgress}
        />

        {reportError && (
          <Alert
            color="red"
            title={t("Board.Analysis.ReportFailed")}
            withCloseButton
            onClose={() => setReportError(null)}
          >
            {t("Board.Analysis.ReportFailed.Desc", { error: reportError })}
          </Alert>
        )}

        {stats.whiteAccuracy > 0 && stats.blackAccuracy > 0 && (
          <Group grow>
            <AccuracyCard
              color={t("Common.WHITE")}
              accuracy={stats.whiteAccuracy}
              cpl={stats.whiteCPL}
            />
            <AccuracyCard
              color={t("Common.BLACK")}
              accuracy={stats.blackAccuracy}
              cpl={stats.blackCPL}
            />
          </Group>
        )}

        <Paper withBorder p="md">
          <EvalChart isAnalysing={inProgress} startAnalysis={openReportingMode} />
        </Paper>

        {commentary && (
          <CommentaryCard
            commentary={commentary}
            facts={facts}
            rootHalfMoves={root.halfMoves}
            onMove={facts ? playCitedMove : undefined}
            onAddToNotes={(text) => appendComment(position, text)}
          />
        )}

        <GameStats {...stats} />
      </Stack>
    </ScrollArea>
  );
}

type Stats = ReturnType<typeof getGameStats>;

const GameStats = memo(
  function GameStats({ whiteAnnotations, blackAnnotations }: Stats) {
    const { t } = useTranslation();

    const store = useContext(TreeStateContext)!;
    const goToAnnotation = useStore(store, (s) => s.goToAnnotation);

    return (
      <Paper withBorder>
        <Grid columns={12} align="center" p="md">
          {Object.keys(ANNOTATION_INFO)
            .filter((a) => isBasicAnnotation(a))
            .map((annotation) => {
              const s = annotation as "??" | "?" | "?!" | "!!" | "!" | "!?";
              const { name, color, translationKey } = ANNOTATION_INFO[s];
              const w = whiteAnnotations[s];
              const b = blackAnnotations[s];
              return (
                <React.Fragment key={annotation}>
                  <Grid.Col
                    className={cx(w > 0 && classes.label)}
                    span={3}
                    style={{ textAlign: "center" }}
                    c={w > 0 ? color : undefined}
                    onClick={() => {
                      if (w > 0) {
                        goToAnnotation(s, "white");
                      }
                    }}
                  >
                    {w}
                  </Grid.Col>
                  <Grid.Col
                    span={2}
                    style={{ textAlign: "center", fontWeight: "bold" }}
                    c={w + b > 0 ? color : undefined}
                  >
                    {annotation}
                  </Grid.Col>
                  <Grid.Col span={4} c={w + b > 0 ? color : undefined}>
                    {translationKey ? t(`Annotate.${translationKey}`) : name}
                  </Grid.Col>
                  <Grid.Col
                    className={cx(b > 0 && classes.label)}
                    span={3}
                    style={{ textAlign: "center" }}
                    c={b > 0 ? color : undefined}
                    onClick={() => {
                      if (b > 0) {
                        goToAnnotation(s, "black");
                      }
                    }}
                  >
                    {b}
                  </Grid.Col>
                </React.Fragment>
              );
            })}
        </Grid>
      </Paper>
    );
  },
  (prev, next) => {
    return (
      equal(prev.whiteAnnotations, next.whiteAnnotations) &&
      equal(prev.blackAnnotations, next.blackAnnotations)
    );
  },
);

function AccuracyCard({ color, cpl, accuracy }: { color: string; cpl: number; accuracy: number }) {
  const { t } = useTranslation();

  return (
    <Paper withBorder p="xs">
      <Group justify="space-between">
        <Stack gap={0} align="start">
          <Text c="dimmed">{color}</Text>
          <Text fz="sm">{cpl.toFixed(1)} ACPL</Text>
        </Stack>
        <Stack gap={0} align="center">
          <Text fz="xl" lh="normal">
            {accuracy.toFixed(1)}%
          </Text>
          <Text fz="sm" c="dimmed" lh="normal">
            {t("Board.Analysis.Accuracy")}
          </Text>
        </Stack>
      </Group>
    </Paper>
  );
}
export default ReportPanel;
