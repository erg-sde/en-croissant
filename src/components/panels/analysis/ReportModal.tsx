import { Button, Checkbox, Group, Modal, NumberInput, Select, Stack } from "@mantine/core";
import { useForm } from "@mantine/form";
import { notifications } from "@mantine/notifications";
import { IconX } from "@tabler/icons-react";
import { getVersion } from "@tauri-apps/api/app";
import { info as logInfo, error as logError } from "@tauri-apps/plugin-log";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { memo, useContext, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type MoveAnalysis, type NarrationRun, type SecretStatus } from "@/bindings";
import { TreeStateContext } from "@/components/common/TreeStateContext";
import {
  commentaryFactsFamily,
  commentaryFactsKey,
  commentarySettingsAtom,
  enginesAtom,
  referenceDbAtom,
  reportSettingsAtom,
  sessionsAtom,
} from "@/state/atoms";
import type { TreeStoreState } from "@/state/store/tree";
import type { NarrationState } from "@/utils/treeReducer";
import { treeRegistry } from "@/state/store/treeRegistry";
import { getMainLine } from "@/utils/chess";
import type { Annotation } from "@/utils/annotation";
import type { GameNarrative } from "@/utils/commentary/types";
import { deriveGlyphs } from "@/utils/commentary/classify";
import { buildNarrationRequest, type NarrationRunContext } from "@/utils/commentary/llm";
import { canNarrate, narratorConfig, narratorHost } from "@/utils/commentary/narrator";
import {
  buildGameNarrative,
  buildMoveCommentary,
  type CommentaryRun,
  type MoveCommentaryResult,
} from "@/utils/commentary/report";
import { pieceWords, type Translate } from "@/utils/commentary/segmentize";
import { resolvePerspective } from "@/utils/commentary/settings";
import { takeMainlineSnapshot } from "@/utils/commentary/snapshot";
import type { LocalEngine } from "@/utils/engines";

// Display string of `Error::AnalysisCancelled` (src-tauri/src/error.rs); command errors cross IPC as
// their message. A cancelled report is not a failure and is never reported.
const ANALYSIS_CANCELLED = "Analysis cancelled";

function ReportModal({
  tab,
  reportingMode,
  closeReportingMode,
  onError,
}: {
  tab: string;
  reportingMode: boolean;
  closeReportingMode: () => void;
  onError: (error: string | null) => void;
}) {
  const { t, i18n } = useTranslation();

  const referenceDb = useAtomValue(referenceDbAtom);
  const sessions = useAtomValue(sessionsAtom);
  const [commentarySettings, setCommentarySettings] = useAtom(commentarySettingsAtom);
  const setCommentaryFacts = useSetAtom(commentaryFactsFamily(tab));
  const engines = useAtomValue(enginesAtom);
  const localEngines = useMemo(
    () => (engines ?? []).filter((e): e is LocalEngine => e.type === "local"),
    [engines],
  );
  const store = useContext(TreeStateContext)!;

  const [reportSettings, setReportSettings] = useAtom(reportSettingsAtom);
  const [secret, setSecret] = useState<SecretStatus | null>(null);
  const [narrate, setNarrate] = useState(true);
  const narrationAvailable = commentarySettings.explain && canNarrate(commentarySettings, secret);

  useEffect(() => {
    if (!reportingMode || !commentarySettings.llmEnabled) return;
    commands.getCommentarySecretStatus(commentarySettings.provider).then((result) => {
      setSecret(result.status === "ok" ? result.data : null);
    });
  }, [reportingMode, commentarySettings.llmEnabled, commentarySettings.provider]);

  const form = useForm({
    initialValues: reportSettings,
    validate: {
      engine: (value) => {
        if (!value) return t("Board.Analysis.EngineRequired");
      },
      novelty: (value) => {
        if (value && !referenceDb) return t("Board.Analysis.RefDBRequired");
      },
    },
  });

  useEffect(() => {
    const engine =
      localEngines.length === 0
        ? ""
        : !reportSettings.engine || !localEngines.some((l) => l.id === reportSettings.engine)
          ? localEngines[0].id
          : reportSettings.engine;

    form.setValues({ ...reportSettings, engine });
  }, [localEngines, reportSettings]);

  function reportFailed(detail: string) {
    notifications.show({
      title: t("Board.Analysis.ReportFailed"),
      message: detail,
      color: "red",
      icon: <IconX />,
    });
    onError(detail);
  }

  function analyze() {
    setReportSettings(form.values);
    onError(null);
    closeReportingMode();
    const engine = localEngines.find((e) => e.id === form.values.engine);
    const engineSettings = (engine?.settings ?? []).map((s) => ({
      ...s,
      value: s.value?.toString() ?? "",
    }));
    const showVariations = form.values.variations;

    // The run is bound to the mainline as it is now. The results are written to the tab's live store
    // when they arrive (it may have been recreated meanwhile), and only onto positions that still
    // match what the engine searched.
    const { root, headers, beginReport } = store.getState();
    const snapshot = takeMainlineSnapshot(root, headers);
    const moves = getMainLine(root);
    const explain = commentarySettings.explain;
    const narrateNow = narrationAvailable && narrate;
    beginReport(snapshot);

    // Deterministic commentary for every analysed move, from the facts the engine pass returned.
    const commentaryFor = async (
      analysis: MoveAnalysis[],
      glyphs: (Annotation | null)[],
    ): Promise<{
      moves: (MoveCommentaryResult | null)[];
      narrative: GameNarrative | null;
      run: CommentaryRun;
    }> => {
      const translate: Translate = (key, options) => String(i18n.t(key, options));
      const run: CommentaryRun = {
        t: translate,
        lang: i18n.language,
        rootHalfMoves: root.halfMoves,
        settings: commentarySettings,
        perspective: resolvePerspective(headers, sessions, commentarySettings.perspective),
        appVersion: await getVersion(),
        pieceWords: pieceWords(translate),
        now: Date.now(),
      };
      const moves = analysis.map((entry, i) =>
        entry.facts ? buildMoveCommentary(entry.facts, glyphs[i], run) : null,
      );
      const narrative = buildGameNarrative(
        analysis.map((entry) => entry.facts ?? null),
        analysis.map((entry) => entry.fen),
        moves.map((result) => result?.selection ?? null),
        headers.result,
        run,
      );
      return { moves, narrative, run };
    };

    // Optional model narration: sentences are upgraded in place as chunks arrive (see
    // CommentaryListener); the deterministic commentary is already written and stays on failure.
    const setNarration = (narration: NarrationState) =>
      treeRegistry.withStore(tab, (live) =>
        live.getState().setNarration(snapshot.runId, narration),
      );
    const startNarration = (context: NarrationRunContext) => {
      logInfo(
        `Narration started: ${context.results.filter((r) => r?.commentary).length} moves with commentary`,
      );
      treeRegistry.startRun(context);
      setNarration({ status: "running" });
      const outcome = (run: NarrationRun): NarrationState =>
        run.status === "failed"
          ? { status: "failed", failure: run.failure }
          : { status: run.status };
      commands
        .narrateGame(
          `report_${tab}`,
          snapshot.runId,
          buildNarrationRequest(context),
          narratorConfig(commentarySettings, i18n.language, context.run.perspective),
        )
        .then((result) => {
          logInfo(
            `Narration finished: ${result.status === "ok" ? JSON.stringify(result.data) : "command error"}`,
          );
          setNarration(result.status === "ok" ? outcome(result.data) : { status: "failed" });
        })
        .catch((e) => {
          logError(`Narration failed: ${e}`);
          setNarration({ status: "failed" });
        });
    };
    const finish = (write?: (live: TreeStoreState) => void) =>
      treeRegistry.withStore(tab, (live) => {
        write?.(live.getState());
        live.getState().endReport(snapshot.runId);
      });

    commands
      .analyzeGame(
        `report_${tab}`,
        engine?.path ?? "",
        form.values.goMode,
        {
          annotateNovelties: form.values.novelty,
          fen: snapshot.rootFen,
          referenceDb,
          reversed: form.values.reversed,
          moves,
          withFacts: explain,
          clocks: snapshot.plies.slice(1).map((ply) => ply.clock ?? null),
          incrementS: snapshot.incrementS,
          initialS: snapshot.initialS,
        },
        engineSettings,
      )
      .then(async (analysis) => {
        if (analysis.status === "ok") {
          const glyphs = deriveGlyphs(analysis.data, snapshot);
          const options = { showVariations, runId: snapshot.runId };
          if (!explain) {
            finish((live) => live.addAnalysis(analysis.data, glyphs, options));
            return;
          }
          const { moves: commentary, narrative, run } = await commentaryFor(analysis.data, glyphs);
          setCommentaryFacts(
            new Map(
              analysis.data.flatMap((entry, i) =>
                entry.facts
                  ? [[commentaryFactsKey(snapshot.rootFen, moves.slice(0, i)), entry.facts]]
                  : [],
              ),
            ),
          );
          finish((live) => live.applyReport(analysis.data, glyphs, options, commentary, narrative));
          if (narrateNow) {
            startNarration({
              tab,
              runId: snapshot.runId,
              provider: commentarySettings.provider,
              run,
              moves: analysis.data.map((entry) => entry.facts ?? null),
              results: commentary,
              narrative,
            });
          }
          return;
        }
        finish();
        if (analysis.error !== ANALYSIS_CANCELLED) {
          reportFailed(analysis.error);
        }
      })
      .catch((e) => {
        finish();
        logError(`Report failed: ${e}`);
        reportFailed(t("Errors.Unknown"));
      });
  }

  return (
    <Modal
      opened={reportingMode}
      onClose={closeReportingMode}
      title={t("Board.Analysis.GenerateReport")}
    >
      <form onSubmit={form.onSubmit(() => analyze())}>
        <Stack>
          <Select
            allowDeselect={false}
            withAsterisk
            label={t("Common.Engine")}
            placeholder="Pick one"
            data={
              localEngines.map((engine) => {
                return {
                  value: engine.id,
                  label: engine.name,
                };
              }) ?? []
            }
            {...form.getInputProps("engine")}
          />
          <Group wrap="nowrap">
            <Select
              allowDeselect={false}
              comboboxProps={{
                position: "bottom",
                middlewares: { flip: false, shift: false },
              }}
              data={[
                { label: t("GoMode.Depth"), value: "Depth" },
                { label: t("Board.Analysis.Time"), value: "Time" },
                { label: t("GoMode.Nodes"), value: "Nodes" },
              ]}
              value={form.values.goMode.t}
              onChange={(v) => {
                const newGo = form.values.goMode;
                newGo.t = v as "Depth" | "Time" | "Nodes";
                form.setFieldValue("goMode", newGo);
              }}
            />
            <NumberInput
              min={1}
              value={form.values.goMode.c as number}
              onChange={(v) =>
                form.setFieldValue("goMode", {
                  ...(form.values.goMode as any),
                  c: (v || 1) as number,
                })
              }
            />
          </Group>

          <Checkbox
            label={t("Board.Analysis.Reversed")}
            description={t("Board.Analysis.Reversed.Desc")}
            {...form.getInputProps("reversed", { type: "checkbox" })}
          />

          <Checkbox
            label={t("Board.Analysis.AnnotateNovelties")}
            description={t("Board.Analysis.AnnotateNovelties.Desc")}
            {...form.getInputProps("novelty", { type: "checkbox" })}
          />

          <Checkbox
            label={t("Board.Analysis.ShowVariations")}
            description={t("Board.Analysis.ShowVariations.Desc")}
            {...form.getInputProps("variations", { type: "checkbox" })}
          />

          <Checkbox
            label={t("Settings.Commentary.Explain")}
            description={t("Settings.Commentary.Explain.Desc")}
            checked={commentarySettings.explain}
            onChange={(event) =>
              setCommentarySettings({ ...commentarySettings, explain: event.currentTarget.checked })
            }
          />

          {commentarySettings.explain && (
            <Select
              label={t("Settings.Commentary.Detail")}
              allowDeselect={false}
              data={[
                { label: t("Settings.Commentary.Detail.KeyMoments"), value: "keyMoments" },
                { label: t("Settings.Commentary.Detail.Mistakes"), value: "mistakes" },
                { label: t("Settings.Commentary.Detail.AllMoves"), value: "allMoves" },
              ]}
              value={commentarySettings.detail}
              onChange={(value) => {
                if (value) {
                  setCommentarySettings({
                    ...commentarySettings,
                    detail: value as typeof commentarySettings.detail,
                  });
                }
              }}
            />
          )}

          {narrationAvailable && (
            <Checkbox
              label={t("Settings.Commentary.NarrateWith", {
                host: narratorHost(commentarySettings),
              })}
              description={t("Settings.Commentary.NarrateWith.Desc")}
              checked={narrate}
              onChange={(event) => setNarrate(event.currentTarget.checked)}
            />
          )}

          <Group justify="right">
            <Button type="submit">{t("Board.Analysis.Analyze")}</Button>
          </Group>
        </Stack>
      </form>
    </Modal>
  );
}

export default memo(ReportModal);
