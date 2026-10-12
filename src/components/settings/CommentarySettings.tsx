import { Select, Switch } from "@mantine/core";
import { useAtom } from "jotai";
import { useTranslation } from "react-i18next";
import { commentarySettingsAtom } from "@/state/atoms";
import type { CommentaryDetail } from "@/utils/commentary/settings";

/** A switch bound to one boolean commentary setting. */
export function CommentarySwitch({ field }: { field: "explain" | "extendedGlyphs" }) {
  const { t } = useTranslation();
  const [settings, setSettings] = useAtom(commentarySettingsAtom);
  return (
    <Switch
      onLabel={t("Common.On")}
      offLabel={t("Common.Off")}
      size="lg"
      checked={settings[field]}
      onChange={(event) => setSettings({ ...settings, [field]: event.currentTarget.checked })}
      styles={{ track: { cursor: "pointer" } }}
    />
  );
}

export function CommentaryDetailSelect() {
  const { t } = useTranslation();
  const [settings, setSettings] = useAtom(commentarySettingsAtom);
  return (
    <Select
      allowDeselect={false}
      data={[
        { label: t("Settings.Commentary.Detail.KeyMoments"), value: "keyMoments" },
        { label: t("Settings.Commentary.Detail.Mistakes"), value: "mistakes" },
        { label: t("Settings.Commentary.Detail.AllMoves"), value: "allMoves" },
      ]}
      value={settings.detail}
      onChange={(value) => {
        if (value) {
          setSettings({ ...settings, detail: value as CommentaryDetail });
        }
      }}
    />
  );
}
