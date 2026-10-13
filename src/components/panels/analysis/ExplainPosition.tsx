import { Button, Group, Stack, Text } from "@mantine/core";
import { useHotkeys } from "@mantine/hooks";
import { IconBulb } from "@tabler/icons-react";
import { getVersion } from "@tauri-apps/api/app";
import { error as logError } from "@tauri-apps/plugin-log";
import { useAtomValue, useSetAtom } from "jotai";
import { memo, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useStore } from "zustand";
import { commands, type MoveFacts, type MoveRef } from "@/bindings";
import { TreeStateContext } from "@/components/common/TreeStateContext";
import {
  activeTabAtom,
  commentaryFactsFamily,
  commentaryFactsKey,
  commentarySettingsAtom,
  enginesAtom,
  reportSettingsAtom,
  sessionsAtom,
} from "@/state/atoms";
import { keyMapAtom } from "@/state/keybinds";
import { getVariationLine } from "@/utils/chess";
import { explainGlyph } from "@/utils/commentary/classify";
import { buildMoveCommentary } from "@/utils/commentary/report";
import { pieceWords, type Translate } from "@/utils/commentary/segmentize";
import { resolvePerspective } from "@/utils/commentary/settings";
import type { MoveCommentary } from "@/utils/commentary/types";
import type { LocalEngine } from "@/utils/engines";
import CommentaryCard from "./CommentaryCard";

// Display string of `Error::AnalysisCancelled`: a newer request replaced this one.
const ANALYSIS_CANCELLED = "Analysis cancelled";

interface Explained {
  path: number[];
  facts: MoveFacts | null;
  commentary: MoveCommentary | null;
}

/**
 * "Explain this position": a short search on a separate engine process (about a second per
 * position), then the same facts, selection, narration and validation as the report, shown as a
 * commentary card that can be kept on the move.
 */
function ExplainPosition() {
  const { t, i18n } = useTranslation();
  const store = useContext(TreeStateContext)!;
  const root = useStore(store, (s) => s.root);
  const headers = useStore(store, (s) => s.headers);
  const position = useStore(store, (s) => s.position);
  const appendComment = useStore(store, (s) => s.appendComment);
  const keepCommentary = useStore(store, (s) => s.keepCommentary);
  const goToMove = useStore(store, (s) => s.goToMove);
  const makeMoves = useStore(store, (s) => s.makeMoves);
  const activeTab = useAtomValue(activeTabAtom);
  const engines = useAtomValue(enginesAtom);
  const reportSettings = useAtomValue(reportSettingsAtom);
  const settings = useAtomValue(commentarySettingsAtom);
  const sessions = useAtomValue(sessionsAtom);
  const keyMap = useAtomValue(keyMapAtom);
  const setFacts = useSetAtom(commentaryFactsFamily(activeTab ?? ""));

  const [loading, setLoading] = useState(false);
  const [explained, setExplained] = useState<Explained | null>(null);
  const [kept, setKept] = useState(false);
  const request = useRef(0);

  const engine = useMemo(() => {
    const local = (engines ?? []).filter((e): e is LocalEngine => e.type === "local");
    return local.find((e) => e.id === reportSettings.engine) ?? local[0] ?? null;
  }, [engines, reportSettings.engine]);

  // An explanation belongs to the move it was made for.
  useEffect(() => {
    if (explained && explained.path.join() !== position.join()) {
      setExplained(null);
      setKept(false);
    }
  }, [position, explained]);

  const explain = useCallback(async () => {
    if (!engine || position.length === 0 || !activeTab) return;
    const id = ++request.current;
    const path = position;
    const moves = getVariationLine(root, path, true);
    setLoading(true);
    setKept(false);
    try {
      const result = await commands.explainPosition(
        `explain_${activeTab}`,
        engine.path,
        { t: "Time", c: 1000 },
        { fen: root.fen, moves, multipv: 3, withThreat: true, threads: 1 },
        (engine.settings ?? []).map((s) => ({ ...s, value: s.value?.toString() ?? "" })),
      );
      if (id !== request.current) return;
      if (result.status === "error") {
        if (result.error !== ANALYSIS_CANCELLED) {
          logError(`Explain failed: ${result.error}`);
          setExplained({ path, facts: null, commentary: null });
        }
        return;
      }
      const { facts, parentLines, lines } = result.data;
      let commentary: MoveCommentary | null = null;
      if (facts) {
        const translate: Translate = (key, options) => String(i18n.t(key, options));
        const glyph = explainGlyph(parentLines, lines, facts.mover, facts.played.san);
        commentary = buildMoveCommentary(facts, glyph, {
          t: translate,
          lang: i18n.language,
          rootHalfMoves: root.halfMoves,
          settings: { detail: "allMoves", extendedGlyphs: settings.extendedGlyphs },
          perspective: resolvePerspective(headers, sessions, settings.perspective),
          appVersion: await getVersion(),
          pieceWords: pieceWords(translate),
          now: Date.now(),
        }).commentary;
        setFacts((map) => new Map(map).set(commentaryFactsKey(root.fen, moves), facts));
      }
      setExplained({ path, facts, commentary });
    } catch (e) {
      logError(`Explain failed: ${e}`);
    } finally {
      if (id === request.current) setLoading(false);
    }
  }, [engine, position, activeTab, root, headers, sessions, settings, i18n, setFacts]);

  useHotkeys([[keyMap.EXPLAIN_POSITION.keys, () => void explain()]]);

  // Plays a move the explanation cites, with the line leading to it, as a variation.
  const playCitedMove = useCallback(
    (ref: MoveRef) => {
      const facts = explained?.facts;
      if (!facts || ref.at.t !== "line") return;
      const line = facts.lines[ref.at.id];
      const start = line.origin === "before" ? position.slice(0, -1) : position;
      goToMove(start);
      makeMoves({ payload: line.pvUci.slice(0, ref.at.ply + 1) });
    },
    [explained, position, goToMove, makeMoves],
  );

  const current = explained && explained.path.join() === position.join() ? explained : null;

  return (
    <Stack gap="xs">
      <Group gap="xs">
        <Button
          size="xs"
          variant="light"
          leftSection={<IconBulb size="0.875rem" />}
          loading={loading}
          disabled={!engine || position.length === 0}
          onClick={() => void explain()}
        >
          {t("Board.Analysis.Explain")}
        </Button>
        {!engine && (
          <Text size="xs" c="dimmed">
            {t("Board.Analysis.Explain.NoEngine")}
          </Text>
        )}
      </Group>
      {current?.commentary && (
        <>
          <CommentaryCard
            commentary={current.commentary}
            facts={current.facts}
            rootHalfMoves={root.halfMoves}
            onMove={playCitedMove}
            onAddToNotes={(text) => appendComment(current.path, text)}
          />
          <Group justify="right">
            <Button
              size="xs"
              variant="default"
              disabled={kept}
              onClick={() => {
                keepCommentary(current.path, current.facts!.fenBefore, current.commentary!);
                setKept(true);
              }}
            >
              {kept ? t("Board.Analysis.Explain.Kept") : t("Board.Analysis.Explain.Keep")}
            </Button>
          </Group>
        </>
      )}
      {current && !current.commentary && (
        <Text size="sm" c="dimmed">
          {t("Board.Analysis.Explain.Nothing")}
        </Text>
      )}
    </Stack>
  );
}

export default memo(ExplainPosition);
