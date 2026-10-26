import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Checkbox, LoadingIndicator, TextField, Button } from "material/react";
import { Banner } from "./ui";
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

export default function AiConnectionSettings() {
  const [draft, setDraft] = useState<ConnectionDraft>();
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    void invoke<AiConnections>("get_uooc_ai_connections")
      .then((connections) => {
        if (!connections?.main || !connections?.jev) throw new Error("AI 连接配置格式无效");
        if (active) setDraft({ ...emptyDraft, connections });
      })
      .catch((cause) => {
        if (active)
          setError(
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
    setMessage("");
    setError("");
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
      setMessage("AI 连接配置已保存");
    } catch (cause) {
      setError(
        typeof cause === "string"
          ? cause
          : cause instanceof Error
            ? cause.message
            : "保存失败，请检查配置",
      );
    } finally {
      setBusy(false);
    }
  };

  if (!draft)
    return (
      <Card variant="outlined">
        <div slot="headline">AI 连接</div>
        <div className="p-4">
          {error ? (
            <Banner status="danger">
              <Banner.Title>无法读取 AI 连接配置</Banner.Title>
              <Banner.Description>{error}</Banner.Description>
            </Banner>
          ) : (
            <p role="status">正在读取 AI 连接配置…</p>
          )}
        </div>
      </Card>
    );

  return (
    <Card variant="outlined">
      <div slot="headline">AI 连接</div>
      <div slot="subhead">API Key 保存在系统凭据存储中，不会回显。</div>
      <div className="p-4">
        <div className="settings-form">
          {error && (
            <Banner status="danger">
              <Banner.Title>AI 连接配置出错</Banner.Title>
              <Banner.Description>{error}</Banner.Description>
            </Banner>
          )}
          {message && (
            <Banner status="success">
              <Banner.Title>{message}</Banner.Title>
            </Banner>
          )}
          {PROVIDERS.map(({ id, title, placeholder }) => (
            <section key={id} className="settings-section">
              <h3 className="settings-section-title">{title}</h3>
              <TextField
                label={id === "main" ? "主模型 API 地址" : "Jev API 地址"}
                value={draft.connections[id].baseUrl}
                placeholder={placeholder}
                disabled={busy}
                onChange={(e: any) => {
                  const value = e.detail?.value ?? e.target?.value ?? "";
                  setDraft(
                    (current) =>
                      current && {
                        ...current,
                        connections: {
                          ...current.connections,
                          [id]: { ...current.connections[id], baseUrl: value },
                        },
                      },
                  );
                }}
              />
              <TextField
                type="password"
                label={id === "main" ? "主模型 API Key" : "Jev API Key"}
                value={draft.keys[id]}
                placeholder={draft.connections[id].hasKey ? "已保存；留空保持不变" : "填写密钥"}
                autocomplete="new-password"
                disabled={busy}
                onChange={(e: any) => {
                  const value = e.detail?.value ?? e.target?.value ?? "";
                  setDraft(
                    (current) =>
                      current && {
                        ...current,
                        keys: { ...current.keys, [id]: value },
                      },
                  );
                }}
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
          <div>
            <Button variant="filled" disabled={busy} onClick={() => void save()}>
              {busy && <LoadingIndicator size={16} />}保存 AI 连接
            </Button>
          </div>
        </div>
      </div>
    </Card>
  );
}
