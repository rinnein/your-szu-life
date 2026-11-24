import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { Button, Card, Dialog, LoadingIndicator, Select, SelectOption } from "material/react";
import { Banner, Chip, SettingList, SettingRow, toast } from "../ui";
import { errorMessage, useAsyncLoad } from "../async-state";
import { usePending } from "../use-pending";
import {
  AI_VENDOR_LABELS,
  aiVendorNeedsKey,
  type AiConfigView,
  type AiProviderView,
} from "./types";
import { ProviderDialog } from "./ProviderDialog";

export default function AiSettingsPage() {
  const { state, reload, apply } = useAsyncLoad<AiConfigView>(() => invoke("get_ai_config"));
  const [editing, setEditing] = useState<AiProviderView | null | undefined>(undefined);
  const [deleting, setDeleting] = useState<AiProviderView | null>(null);
  const [defaultProviderId, setDefaultProviderId] = useState("");
  const [defaultModel, setDefaultModel] = useState("");
  const [resetOpen, setResetOpen] = useState(false);
  const pending = usePending();

  const config =
    state.status === "ready" ? state.data : state.status === "loading" ? state.previous : undefined;
  const defaultProviderIdLive = config?.providers.some(
    (provider) => provider.id === defaultProviderId,
  )
    ? defaultProviderId
    : (config?.default?.providerId ?? config?.providers[0]?.id ?? "");
  const defaultProvider = config?.providers.find(
    (provider) => provider.id === defaultProviderIdLive,
  );

  useEffect(() => {
    if (!config) return;
    setDefaultProviderId(defaultProviderIdLive);
    const models = defaultProvider?.models ?? [];
    if (
      config.default?.providerId === defaultProviderIdLive &&
      models.some((model) => model.id === config.default?.model)
    ) {
      setDefaultModel(config.default.model);
    } else {
      setDefaultModel(models[0]?.id ?? "");
    }
  }, [config, defaultProviderIdLive, defaultProvider]);

  const saveDefault = async () => {
    if (!defaultProviderIdLive) {
      toast.danger("请先添加 AI 提供商");
      return;
    }
    if (!defaultModel.trim()) {
      toast.danger("请选择默认模型");
      return;
    }
    const outcome = await pending.run("default", () =>
      invoke<AiConfigView>("set_ai_default", {
        input: { providerId: defaultProviderIdLive, model: defaultModel },
      }),
    );
    if (!outcome.ok) {
      if (!outcome.skipped) {
        toast.danger("保存默认模型失败", { description: errorMessage(outcome.error) });
      }
      return;
    }
    apply(outcome.value);
    toast.success("默认模型已保存");
  };

  const remove = async () => {
    if (!deleting) return;
    const outcome = await pending.run(`delete:${deleting.id}`, () =>
      invoke<AiConfigView>("remove_ai_provider", { providerId: deleting.id }),
    );
    if (!outcome.ok) {
      if (!outcome.skipped) {
        toast.danger("删除 AI 提供商失败", { description: errorMessage(outcome.error) });
      }
      return;
    }
    apply(outcome.value);
    setDeleting(null);
    toast.success("AI 提供商已删除");
  };

  const resetConfig = async () => {
    const outcome = await pending.run("reset", () => invoke<AiConfigView>("reset_ai_config"));
    if (!outcome.ok) {
      if (!outcome.skipped) {
        toast.danger("重置 AI 配置失败", { description: errorMessage(outcome.error) });
      }
      return;
    }
    apply(outcome.value);
    setResetOpen(false);
    toast.success("AI 配置已重置");
  };

  if (!config) {
    return state.status === "error" ? (
      <>
        <Banner status="danger">
          <Banner.Title>无法读取 AI 配置</Banner.Title>
          <Banner.Description>{state.message}</Banner.Description>
        </Banner>
        <div className="flex flex-wrap gap-2">
          <Button variant="outlined" onClick={() => void reload()}>
            重新读取
          </Button>
          <Button
            variant="tonal"
            disabled={pending.isPending("reset")}
            onClick={() => setResetOpen(true)}
          >
            重置 AI 配置
          </Button>
        </div>
        {resetOpen &&
          createPortal(
            <Dialog
              open
              headline="重置 AI 配置？"
              onClose={() => setResetOpen(false)}
              actions={
                <>
                  <Button variant="text" onClick={() => setResetOpen(false)}>
                    取消
                  </Button>
                  <Button
                    variant="filled"
                    disabled={pending.isPending("reset")}
                    onClick={() => void resetConfig()}
                  >
                    {pending.isPending("reset") && <LoadingIndicator size={16} />}
                    重置
                  </Button>
                </>
              }
            >
              <p>将删除所有 AI 提供商及其保存的 API Key。此操作不可撤销。</p>
            </Dialog>,
            document.body,
          )}
      </>
    ) : (
      <p className="status-line" role="status">
        <LoadingIndicator size={16} aria-hidden="true" />
        正在读取 AI 配置…
      </p>
    );
  }

  const isDefault = (provider: AiProviderView) => config.default?.providerId === provider.id;

  return (
    <>
      <Card variant="outlined">
        <div slot="headline">默认模型</div>
        <div slot="subhead">
          UOOC 的 AI 答题会使用这里的模型。题库匹配后仍有未确定题目时，会通过应用内 ACP
          答题子部件处理。
        </div>
        {config.providers.length === 0 ? (
          <div className="grid gap-3">
            <Banner status="warning">
              <Banner.Title>尚未配置 AI 提供商</Banner.Title>
              <Banner.Description>
                添加一个提供商并选择默认模型后，才能使用 UOOC AI 答题。手动答题不受影响。
              </Banner.Description>
            </Banner>
            <div>
              <Button variant="filled" onClick={() => setEditing(null)}>
                添加提供商
              </Button>
            </div>
          </div>
        ) : (
          <div className="grid gap-3">
            <Select
              aria-label="默认 AI 提供商"
              label="默认提供商"
              value={defaultProviderIdLive}
              disabled={pending.isPending("default")}
              onChange={(event: any) => {
                const value = event.target?.value ?? event.detail?.value ?? "";
                setDefaultProviderId(value);
                const provider = config.providers.find((item) => item.id === value);
                setDefaultModel(provider?.models[0]?.id ?? "");
              }}
            >
              {config.providers.map((provider) => (
                <SelectOption key={provider.id} value={provider.id}>
                  {provider.name}
                </SelectOption>
              ))}
            </Select>
            {defaultProvider && defaultProvider.models.length > 0 ? (
              <Select
                aria-label="默认模型"
                label="默认模型"
                value={
                  defaultProvider.models.some((model) => model.id === defaultModel)
                    ? defaultModel
                    : ""
                }
                disabled={pending.isPending("default")}
                onChange={(event: any) => {
                  const value = event.target?.value ?? event.detail?.value;
                  if (typeof value === "string" && value) setDefaultModel(value);
                }}
              >
                {defaultProvider.models.map((model) => (
                  <SelectOption key={model.id} value={model.id}>
                    {model.name || model.id}
                  </SelectOption>
                ))}
              </Select>
            ) : (
              <Banner status="warning">
                <Banner.Title>该提供商还没有模型</Banner.Title>
                <Banner.Description>
                  请先在「模型提供商」里编辑它并添加模型，能力会自动匹配。
                </Banner.Description>
              </Banner>
            )}
            <div>
              <Button
                variant="filled"
                disabled={pending.isPending("default") || !defaultModel}
                onClick={() => void saveDefault()}
              >
                {pending.isPending("default") && <LoadingIndicator size={16} />}
                保存默认模型
              </Button>
            </div>
          </div>
        )}
      </Card>

      <Card variant="outlined">
        <div slot="headline">模型提供商</div>
        <div slot="subhead">
          配置结构与 VS Code chatLanguageModels.json 对齐，可按供应商与模型名自动匹配
          toolCalling、vision、上下文与输出限制。API Key 保存在系统凭据存储中，不会回显。
        </div>
        {config.providers.length > 0 && (
          <SettingList>
            {config.providers.map((provider) => {
              const modelNames = provider.models.map((model) => model.name || model.id);
              return (
                <SettingRow
                  key={provider.id}
                  title={
                    <span className="flex flex-wrap items-center gap-2">
                      {provider.name}
                      {isDefault(provider) && <Chip color="accent">默认</Chip>}
                      {provider.builtin && <Chip>内置 · 锁定</Chip>}
                      {aiVendorNeedsKey(provider.vendor) && !provider.hasKey ? (
                        <Chip color="warning">未设置 Key</Chip>
                      ) : null}
                    </span>
                  }
                  description={
                    <span className="break-all">
                      {AI_VENDOR_LABELS[provider.vendor]} · {provider.baseUrl}
                      {modelNames.length > 0 ? ` · ${modelNames.join("、")}` : " · 无模型"}
                    </span>
                  }
                >
                  {provider.builtin ? (
                    <span className="text-xs text-muted">
                      Dify 公共聊天由应用内置，分享码与文本能力均锁定
                    </span>
                  ) : (
                    <div className="flex flex-wrap gap-2">
                      <Button variant="text" onClick={() => setEditing(provider)}>
                        编辑
                      </Button>
                      <Button
                        variant="text"
                        disabled={pending.isPending(`delete:${provider.id}`)}
                        onClick={() => setDeleting(provider)}
                      >
                        删除
                      </Button>
                    </div>
                  )}
                </SettingRow>
              );
            })}
          </SettingList>
        )}
        <div className="mt-3">
          <Button variant="tonal" onClick={() => setEditing(null)}>
            添加提供商
          </Button>
        </div>
      </Card>

      {editing !== undefined && (
        <ProviderDialog
          provider={editing}
          defaultModel={
            editing && config.default?.providerId === editing.id ? config.default.model : undefined
          }
          onClose={() => setEditing(undefined)}
          onSaved={(next) => {
            apply(next);
            setEditing(undefined);
          }}
        />
      )}

      {deleting &&
        createPortal(
          <Dialog
            open
            headline="删除 AI 提供商？"
            onClose={() => setDeleting(null)}
            actions={
              <>
                <Button variant="text" onClick={() => setDeleting(null)}>
                  取消
                </Button>
                <Button
                  variant="filled"
                  disabled={pending.isPending(`delete:${deleting.id}`)}
                  onClick={() => void remove()}
                >
                  {pending.isPending(`delete:${deleting.id}`) && <LoadingIndicator size={16} />}
                  删除
                </Button>
              </>
            }
          >
            <p>
              将删除「{deleting.name}」及其保存的密钥。
              {isDefault(deleting)
                ? "它当前是默认提供商，删除后会回退到内置的 UOOC 智能小优。"
                : ""}
            </p>
          </Dialog>,
          document.body,
        )}
    </>
  );
}
