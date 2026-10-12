import { Alert, Button, Collapse, Group, Loader, Text } from "@mantine/core";
import { memo, useState } from "react";
import { useTranslation } from "react-i18next";
import { narrationFailureMessage } from "@/utils/commentary/errors";
import type { Translate } from "@/utils/commentary/segmentize";
import type { NarrationState } from "@/utils/treeReducer";

/**
 * The state of the model narration of the current report: a cancellable "narrating" line while it
 * runs, and an inline alert when it fails (the deterministic commentary stays). Provider detail is
 * redacted in Rust and shown only on request.
 */
function NarrationStatus({
  narration,
  onCancel,
}: {
  narration: NarrationState | undefined;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [showDetail, setShowDetail] = useState(false);
  if (!narration) return null;

  if (narration.status === "running") {
    return (
      <Group gap="xs">
        <Loader size="xs" />
        <Text size="sm" c="dimmed">
          {t("Commentary.Narrative.Narrating")}
        </Text>
        <Button size="compact-xs" variant="subtle" onClick={onCancel}>
          {t("Common.Cancel")}
        </Button>
      </Group>
    );
  }
  if (narration.status !== "failed") return null;

  const translate = t as unknown as Translate;
  const message = narration.failure
    ? narrationFailureMessage(narration.failure, translate)
    : { title: t("Errors.Unknown"), detail: null };
  if (!message) return null;
  return (
    <Alert color="orange" title={t("Errors.Commentary.Failed")}>
      <Text size="sm">{message.title}</Text>
      {message.detail && (
        <>
          <Button
            size="compact-xs"
            variant="subtle"
            px={0}
            onClick={() => setShowDetail((v) => !v)}
          >
            {t("Errors.Commentary.Details")}
          </Button>
          <Collapse in={showDetail}>
            <Text size="xs" c="dimmed" style={{ userSelect: "text" }}>
              {message.detail}
            </Text>
          </Collapse>
        </>
      )}
    </Alert>
  );
}

export default memo(NarrationStatus);
