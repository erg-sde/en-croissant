import { Button, Checkbox, Group, Modal, NumberInput, Select, Stack } from "@mantine/core";
import { useForm } from "@mantine/form";
import { notifications } from "@mantine/notifications";
import { IconX } from "@tabler/icons-react";
import { error as logError } from "@tauri-apps/plugin-log";
import { useAtom, useAtomValue } from "jotai";
import { memo, useContext, useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { TreeStateContext } from "@/components/common/TreeStateContext";
import { enginesAtom, referenceDbAtom, reportSettingsAtom } from "@/state/atoms";
import type { TreeStoreState } from "@/state/store/tree";
import { treeRegistry } from "@/state/store/treeRegistry";
import { getMainLine } from "@/utils/chess";
import { deriveGlyphs } from "@/utils/commentary/classify";
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
  const { t } = useTranslation();

  const referenceDb = useAtomValue(referenceDbAtom);
  const engines = useAtomValue(enginesAtom);
  const localEngines = useMemo(
    () => (engines ?? []).filter((e): e is LocalEngine => e.type === "local"),
    [engines],
  );
  const store = useContext(TreeStateContext)!;

  const [reportSettings, setReportSettings] = useAtom(reportSettingsAtom);

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
    beginReport(snapshot);
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
          moves: getMainLine(root),
        },
        engineSettings,
      )
      .then((analysis) => {
        if (analysis.status === "ok") {
          const glyphs = deriveGlyphs(analysis.data, snapshot);
          finish((live) =>
            live.addAnalysis(analysis.data, glyphs, { showVariations, runId: snapshot.runId }),
          );
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

          <Group justify="right">
            <Button type="submit">{t("Board.Analysis.Analyze")}</Button>
          </Group>
        </Stack>
      </form>
    </Modal>
  );
}

export default memo(ReportModal);
