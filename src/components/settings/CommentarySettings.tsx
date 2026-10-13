import {
  Button,
  Group,
  List,
  Modal,
  PasswordInput,
  Select,
  Stack,
  Switch,
  Text,
  TextInput,
} from "@mantine/core";
import { useAtom } from "jotai";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type SecretStatus } from "@/bindings";
import { commentarySettingsAtom } from "@/state/atoms";
import { narrationFailureMessage } from "@/utils/commentary/errors";
import { CONSENT_VERSION, narratorConfig, narratorHost } from "@/utils/commentary/narrator";
import type { Translate } from "@/utils/commentary/segmentize";
import type {
  CommentaryDetail,
  CommentarySettings as CommentarySettingsType,
} from "@/utils/commentary/settings";

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

export function CommentaryPerspectiveSelect() {
  const { t } = useTranslation();
  const [settings, setSettings] = useAtom(commentarySettingsAtom);
  return (
    <Select
      allowDeselect={false}
      data={[
        { label: t("Settings.Commentary.Perspective.Auto"), value: "auto" },
        { label: t("Settings.Commentary.Perspective.White"), value: "white" },
        { label: t("Settings.Commentary.Perspective.Black"), value: "black" },
        { label: t("Settings.Commentary.Perspective.Both"), value: "both" },
      ]}
      value={settings.perspective}
      onChange={(value) => {
        if (value) {
          setSettings({
            ...settings,
            perspective: value as CommentarySettingsType["perspective"],
          });
        }
      }}
    />
  );
}

/**
 * Turns model narration on. The first time, the consent modal lists exactly what is sent and where;
 * nothing is enabled until it is accepted.
 */
export function NarrationSwitch() {
  const { t } = useTranslation();
  const [settings, setSettings] = useAtom(commentarySettingsAtom);
  const [consentOpen, setConsentOpen] = useState(false);
  return (
    <>
      <Switch
        onLabel={t("Common.On")}
        offLabel={t("Common.Off")}
        size="lg"
        checked={settings.llmEnabled}
        onChange={(event) => {
          if (!event.currentTarget.checked) {
            setSettings({ ...settings, llmEnabled: false });
          } else if (settings.consentVersion >= CONSENT_VERSION) {
            setSettings({ ...settings, llmEnabled: true });
          } else {
            setConsentOpen(true);
          }
        }}
        styles={{ track: { cursor: "pointer" } }}
      />
      <NarrationConsentModal
        opened={consentOpen}
        host={narratorHost(settings) || t("Settings.Commentary.Consent.YourServer")}
        anthropic={settings.provider === "anthropic"}
        onClose={() => setConsentOpen(false)}
        onAccept={() => {
          setSettings({ ...settings, llmEnabled: true, consentVersion: CONSENT_VERSION });
          setConsentOpen(false);
        }}
      />
    </>
  );
}

function NarrationConsentModal({
  opened,
  host,
  anthropic,
  onClose,
  onAccept,
}: {
  opened: boolean;
  host: string;
  anthropic: boolean;
  onClose: () => void;
  onAccept: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title={t("Settings.Commentary.Consent.Title")}
      size="lg"
    >
      <Stack gap="sm">
        <Text size="sm">{t("Settings.Commentary.Consent.Sent")}</Text>
        <List size="sm">
          <List.Item>{t("Settings.Commentary.Consent.SentPositions")}</List.Item>
          <List.Item>{t("Settings.Commentary.Consent.SentEngine")}</List.Item>
          <List.Item>{t("Settings.Commentary.Consent.SentFacts")}</List.Item>
          <List.Item>{t("Settings.Commentary.Consent.SentDrafts")}</List.Item>
          <List.Item>{t("Settings.Commentary.Consent.SentLanguage")}</List.Item>
        </List>
        <Text size="sm">{t("Settings.Commentary.Consent.NeverSent")}</Text>
        <Text size="sm">{t("Settings.Commentary.Consent.Direct", { host })}</Text>
        {anthropic && <Text size="sm">{t("Settings.Commentary.Consent.Fallback")}</Text>}
        <Text size="sm" c="dimmed">
          {t("Settings.Commentary.Consent.Optional")}
        </Text>
        <Group justify="right">
          <Button variant="default" onClick={onClose}>
            {t("Common.Cancel")}
          </Button>
          <Button onClick={onAccept}>{t("Settings.Commentary.Consent.Accept")}</Button>
        </Group>
      </Stack>
    </Modal>
  );
}

/** Provider, model, server and key. The key is sent to Rust once and never read back. */
export function NarratorSettings() {
  const { t, i18n } = useTranslation();
  const [settings, setSettings] = useAtom(commentarySettingsAtom);
  const [status, setStatus] = useState<SecretStatus | null>(null);
  const [key, setKey] = useState("");
  const [baseUrl, setBaseUrl] = useState(settings.baseUrl ?? "");
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string } | null>(null);

  const refresh = useCallback(() => {
    commands.getCommentarySecretStatus(settings.provider).then((result) => {
      setStatus(result.status === "ok" ? result.data : null);
    });
  }, [settings.provider]);
  useEffect(refresh, [refresh]);
  useEffect(() => setBaseUrl(settings.baseUrl ?? ""), [settings.baseUrl]);

  const baseUrlValid = baseUrl.trim() === "" || URL.canParse(baseUrl.trim());

  async function saveKey() {
    const secret = key;
    setKey("");
    await commands.setCommentarySecret(settings.provider, secret);
    refresh();
  }

  async function clearKey() {
    await commands.clearCommentarySecret(settings.provider);
    refresh();
  }

  async function testConnection() {
    setTesting(true);
    setTestResult(null);
    const translate: Translate = (k, options) => String(i18n.t(k, options));
    const result = await commands.testNarrator(narratorConfig(settings, i18n.language, "both"));
    setTesting(false);
    if (result.status === "error") {
      setTestResult({ ok: false, text: t("Errors.Unknown") });
    } else if (result.data === null) {
      setTestResult({ ok: true, text: t("Settings.Commentary.Test.Ok") });
    } else {
      const message = narrationFailureMessage(result.data, translate);
      setTestResult({
        ok: false,
        text: message ? [message.title, message.detail].filter(Boolean).join(" — ") : "",
      });
    }
  }

  const statusText = status?.configured
    ? t("Settings.Commentary.Key.Configured", { hint: status.hint ?? "…" })
    : settings.provider === "openaiCompatible" && settings.baseUrl
      ? t("Settings.Commentary.Key.LocalServer")
      : t("Settings.Commentary.Key.Missing");

  return (
    <Stack gap="xs" w={360}>
      <Select
        label={t("Settings.Commentary.Provider")}
        allowDeselect={false}
        data={[
          { label: "Anthropic", value: "anthropic" },
          { label: t("Settings.Commentary.Provider.OpenaiCompatible"), value: "openaiCompatible" },
        ]}
        value={settings.provider}
        onChange={(value) => {
          if (value) {
            setSettings({ ...settings, provider: value as CommentarySettingsType["provider"] });
            setTestResult(null);
          }
        }}
      />
      <TextInput
        label={t("Settings.Commentary.Model")}
        value={settings.model}
        onChange={(event) => setSettings({ ...settings, model: event.currentTarget.value })}
      />
      <TextInput
        label={t("Settings.Commentary.BaseUrl")}
        description={t("Settings.Commentary.BaseUrl.Desc")}
        placeholder={
          settings.provider === "anthropic" ? "https://api.anthropic.com" : "http://localhost:11434"
        }
        value={baseUrl}
        error={baseUrlValid ? undefined : t("Settings.Commentary.BaseUrl.Invalid")}
        onChange={(event) => setBaseUrl(event.currentTarget.value)}
        onBlur={() => {
          const trimmed = baseUrl.trim();
          if (trimmed === "") setSettings({ ...settings, baseUrl: null });
          else if (URL.canParse(trimmed)) setSettings({ ...settings, baseUrl: trimmed });
        }}
      />
      <Group align="end" gap="xs" wrap="nowrap">
        <PasswordInput
          style={{ flex: 1 }}
          label={t("Settings.Commentary.Key")}
          value={key}
          autoComplete="off"
          onChange={(event) => setKey(event.currentTarget.value)}
        />
        <Button disabled={key.trim() === ""} onClick={saveKey}>
          {t("Common.Save")}
        </Button>
        <Button variant="default" disabled={!status?.configured} onClick={clearKey}>
          {t("Settings.Commentary.Key.Clear")}
        </Button>
      </Group>
      <Text size="xs" c="dimmed">
        {statusText}
      </Text>
      <Group gap="xs">
        <Button variant="light" loading={testing} onClick={testConnection}>
          {t("Settings.Commentary.Test")}
        </Button>
        {testResult && (
          <Text size="xs" c={testResult.ok ? "teal" : "red"}>
            {testResult.text}
          </Text>
        )}
      </Group>
    </Stack>
  );
}
