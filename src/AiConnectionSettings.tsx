import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Card, Checkbox, Input, Label, Spinner, TextField } from "@heroui/react";
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
      .catch((error) => {
        if (active)
          setError(
            typeof error === "string"
              ? error
              : error instanceof Error
                ? error.message
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
    } catch (error) {
      setError(
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "保存失败，请检查配置",
      );
    } finally {
      setBusy(false);
    }
  };

  if (!draft)
    return (
      <Card>
        <Card.Header>
          <Card.Title>AI 连接</Card.Title>
        </Card.Header>
        <Card.Content>
          {error ? (
            <Alert status="danger" role="alert">
              <Alert.Content>
                <Alert.Title>无法读取 AI 连接配置</Alert.Title>
                <Alert.Description>{error}</Alert.Description>
              </Alert.Content>
            </Alert>
          ) : (
            <p role="status">正在读取 AI 连接配置…</p>
          )}
        </Card.Content>
      </Card>
    );

  return (
    <Card>
      <Card.Header>
        <Card.Title>AI 连接</Card.Title>
        <Card.Description>API Key 保存在系统凭据存储中，不会回显。</Card.Description>
      </Card.Header>
      <Card.Content className="grid gap-5">
        {error && (
          <Alert status="danger" role="alert">
            <Alert.Content>
              <Alert.Title>AI 连接配置出错</Alert.Title>
              <Alert.Description>{error}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {message && (
          <Alert status="success" role="status">
            <Alert.Content>
              <Alert.Title>{message}</Alert.Title>
            </Alert.Content>
          </Alert>
        )}
        {(["main", "jev"] as const).map((provider) => (
          <section key={provider} className="grid gap-3">
            <h3 className="font-semibold">
              {provider === "main" ? "主模型 · OpenAI 兼容接口" : "Jev · TypeSafe System One"}
            </h3>
            <TextField
              value={draft.connections[provider].baseUrl}
              onChange={(baseUrl) =>
                setDraft(
                  (current) =>
                    current && {
                      ...current,
                      connections: {
                        ...current.connections,
                        [provider]: { ...current.connections[provider], baseUrl },
                      },
                    },
                )
              }
              isDisabled={busy}
            >
              <Label>{provider === "main" ? "主模型" : "Jev"} API 地址</Label>
              <Input
                placeholder={
                  provider === "main" ? "https://example.com/v1" : "https://api.typesafe.ai/v1"
                }
              />
            </TextField>
            <TextField
              type="password"
              value={draft.keys[provider]}
              onChange={(value) =>
                setDraft(
                  (current) =>
                    current && {
                      ...current,
                      keys: { ...current.keys, [provider]: value },
                    },
                )
              }
              isDisabled={busy}
            >
              <Label>{provider === "main" ? "主模型" : "Jev"} API Key</Label>
              <Input
                autoComplete="new-password"
                placeholder={
                  draft.connections[provider].hasKey ? "已保存；留空保持不变" : "填写密钥"
                }
              />
            </TextField>
            <Checkbox
              isSelected={draft.clear[provider]}
              onChange={(clearKey) =>
                setDraft(
                  (current) =>
                    current && {
                      ...current,
                      clear: { ...current.clear, [provider]: clearKey },
                    },
                )
              }
              isDisabled={busy}
            >
              <Checkbox.Control>
                <Checkbox.Indicator />
              </Checkbox.Control>
              <Checkbox.Content>
                删除已保存的{provider === "main" ? "主模型" : "Jev"}密钥
              </Checkbox.Content>
            </Checkbox>
          </section>
        ))}
        <Button isDisabled={busy} onPress={() => void save()}>
          {busy && <Spinner size="sm" color="current" />}保存 AI 连接
        </Button>
      </Card.Content>
    </Card>
  );
}
