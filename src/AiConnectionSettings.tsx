import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Checkbox, LoadingIndicator, TextField, Button } from "material/react";
import { Banner, bindField, toast } from "./ui";
import { useRegisterNextAction, type StepNextAction } from "./step-next-action";
import type { AiConnections } from "./uooc/types";

type ConnectionDraft = {
  connections: AiConnections;
  keys: { main: string; jev: string };
  clear: { main: boolean; jev: boolean };
};

const emptyDraft: ConnectionDraft = {
  connections: {
    main: { baseUrl: "", hasKey: false },
    jev: { baseUrl: "https://api.typesafe.ai/v1", hasKey: false },
  },
  keys: { main: "", jev: "" },
  clear: { main: false, jev: false },
};

const PROVIDERS = [
  { id: "main" as const, title: "主模型 · OpenAI 兼容接口", placeholder: "https://example.com/v1" },
  {
    id: "jev" as const,
    title: "Jev · TypeSafe System One",
    placeholder: "https://api.typesafe.ai/v1",
  },
];

export function AiConnectionForm({
  showJev = true,
  submitLabel = "保存 AI 连接",
  onSaved,
  onboarding = false,
  registerNext,
}: {
  showJev?: boolean;
  submitLabel?: string;
  onSaved?: (connections: AiConnections) => void | Promise<void>;
  onboarding?: boolean;
  registerNext?: (action: StepNextAction | null) => void;
}) {
  const [draft, setDraft] = useState<ConnectionDraft>();
  const [busy, setBusy] = useState(false);
  const [loadError, setLoadError] = useState("");

  useEffect(() => {
    let active = true;
    void invoke<AiConnections>("get_uooc_ai_connections")
      .then((connections) => {
        if (!connections?.main || !connections?.jev) throw new Error("AI 连接配置格式无效");
        if (active) setDraft({ ...emptyDraft, connections });
      })
      .catch((cause) => {
        if (active)
          setLoadError(
            typeof cause === "string"
              ? cause
              : cause instanceof Error
                ? cause.message
                : "无法读取 AI 连接配置",
          );
      });
    return () => {
      active = false;
    };
  }, []);

  const save = async () => {
    if (!draft) return;
    setBusy(true);
    try {
      const connections = await invoke<AiConnections>("save_uooc_ai_connections", {
        connections: {
          main: {
            baseUrl: draft.connections.main.baseUrl,
            apiKey: draft.keys.main || null,
            clearKey: draft.clear.main,
          },
          jev: {
            baseUrl: draft.connections.jev.baseUrl,
            apiKey: draft.keys.jev || null,
            clearKey: draft.clear.jev,
          },
        },
      });
      setDraft({
        ...draft,
        connections,
        keys: { main: "", jev: "" },
        clear: { main: false, jev: false },
      });
      toast.success("AI 连接配置已保存");
      try {
        await onSaved?.(connections);
      } catch (callbackError) {
        // 配置已经保存成功；后续步骤失败由调用方展示，不要误报为保存失败。
        console.error("AI 配置保存后的回调失败", callbackError);
      }
    } catch (cause) {
      toast.danger("保存 AI 连接失败", {
        description:
          typeof cause === "string"
            ? cause
            : cause instanceof Error
              ? cause.message
              : "保存失败，请检查配置",
      });
    } finally {
      setBusy(false);
    }
  };

  useRegisterNextAction(
    registerNext,
    onboarding && draft ? { label: submitLabel, disabled: busy, busy, run: save } : null,
  );

  if (!draft)
    return (
      <div className="grid gap-3">
        {loadError ? (
          <Banner status="danger">
            <Banner.Title>无法读取 AI 连接配置</Banner.Title>
            <Banner.Description>{loadError}</Banner.Description>
          </Banner>
        ) : (
          <p role="status">正在读取 AI 连接配置…</p>
        )}
      </div>
    );

  const providers = PROVIDERS.filter((provider) => showJev || provider.id === "main");

  return (
    <div className="settings-form">
      {providers.map(({ id, title, placeholder }) => (
        <section key={id} className="settings-section">
          <h3 className="settings-section-title">{title}</h3>
          <TextField
            label={id === "main" ? "主模型 API 地址" : "Jev API 地址"}
            value={draft.connections[id].baseUrl}
            placeholder={placeholder}
            disabled={busy}
            {...bindField((value) =>
              setDraft(
                (current) =>
                  current && {
                    ...current,
                    connections: {
                      ...current.connections,
                      [id]: { ...current.connections[id], baseUrl: value },
                    },
                  },
              ),
            )}
          />
          <TextField
            type="password"
            label={id === "main" ? "主模型 API Key" : "Jev API Key"}
            value={draft.keys[id]}
            placeholder={draft.connections[id].hasKey ? "已保存；留空保持不变" : "填写密钥"}
            autocomplete="new-password"
            disabled={busy}
            {...bindField((value) =>
              setDraft(
                (current) =>
                  current && {
                    ...current,
                    keys: { ...current.keys, [id]: value },
                  },
              ),
            )}
          />
          <Checkbox
            checked={draft.clear[id]}
            disabled={busy}
            onChange={(event: any) =>
              setDraft(
                (current) =>
                  current && {
                    ...current,
                    clear: {
                      ...current.clear,
                      [id]: Boolean(event.detail?.checked ?? event.target?.checked),
                    },
                  },
              )
            }
          >
            删除已保存的{id === "main" ? "主模型" : "Jev"}密钥
          </Checkbox>
        </section>
      ))}
      {!onboarding && (
        <div>
          <Button variant="filled" disabled={busy} onClick={() => void save()}>
            {busy && <LoadingIndicator size={16} />}
            {busy ? "保存中…" : submitLabel}
          </Button>
        </div>
      )}
    </div>
  );
}

export default function AiConnectionSettings({
  onboarding = false,
  onComplete,
  registerNext,
}: {
  onboarding?: boolean;
  onComplete?: () => void | Promise<void>;
  registerNext?: (action: StepNextAction | null) => void;
}) {
  if (onboarding) {
    return (
      <div className="grid gap-4">
        <Banner status="info">
          <Banner.Title>未配置 AI 时无法使用 AI 功能</Banner.Title>
          <Banner.Description>
            例如 UOOC 的 AI 答题与解析。Jev 是可选的选择题复核模型，不配置也不影响基本 AI
            功能；这里只需要填写主模型的地址和 Key。
          </Banner.Description>
        </Banner>
        <Card variant="outlined">
          <div slot="headline">AI 配置</div>
          <div slot="subhead">只填写主模型即可；Jev 可以在设置 → AI 配置中按需开启。</div>
          <div className="p-4">
            <AiConnectionForm
              onboarding
              showJev={false}
              submitLabel="保存并继续"
              registerNext={registerNext}
              onSaved={async (connections) => {
                if (!connections.main.baseUrl || !connections.main.hasKey) {
                  toast.info("主模型尚未配置完整；可以继续填写，或跳过此步骤");
                  return;
                }
                await onComplete?.();
              }}
            />
          </div>
        </Card>
      </div>
    );
  }

  return (
    <Card variant="outlined">
      <div slot="headline">AI 连接</div>
      <div slot="subhead">API Key 保存在系统凭据存储中，不会回显。</div>
      <div className="p-4">
        <AiConnectionForm />
      </div>
    </Card>
  );
}
