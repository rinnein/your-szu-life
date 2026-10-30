import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  Card,
  Dialog,
  LoadingIndicator,
  Switch,
  TextField,
  Button,
  Tab,
  Tabs,
} from "material/react";
import { Banner, bindField, toast } from "./ui";
import { nameTablist } from "./ui/tablist";
import {
  staffModeText,
  type StaffApplyResult,
  type StaffCredentialMode,
  type StaffLoginMode,
  type StaffSettings,
  type StaffSettingsInput,
  type StaffStatus,
} from "./staff-network";

type Props = {
  settings: StaffSettings;
  status: StaffStatus;
  disabled: boolean;
  onSave: (input: StaffSettingsInput) => Promise<StaffSettings>;
  onApply: (confirmationToken?: string) => Promise<StaffApplyResult>;
};

export default function StaffNetworkPanel({ settings, status, disabled, onSave, onApply }: Props) {
  const [mode, setMode] = useState<StaffLoginMode>(settings.mode);
  const [enabled, setEnabled] = useState(settings.enabled);
  const [credentialMode, setCredentialMode] = useState<StaffCredentialMode>(
    settings.credentialMode,
  );
  const [card, setCard] = useState(settings.customCampusCardNumber ?? "");
  const [password, setPassword] = useState("");
  const [saving, setSaving] = useState(false);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState("");
  const [feedback, setFeedback] = useState<StaffApplyResult>();
  const [confirmation, setConfirmation] = useState<StaffApplyResult>();
  const mounted = useRef(true);
  const applyingRef = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    setMode(settings.mode);
    setEnabled(settings.enabled);
    setCredentialMode(settings.credentialMode);
    setCard(settings.customCampusCardNumber ?? "");
    setConfirmation(undefined);
  }, [
    settings.mode,
    settings.enabled,
    settings.credentialMode,
    settings.revision,
    settings.customCampusCardNumber,
  ]);
  const inProgress = applying || status.state === "probing" || status.state === "loggingIn";
  const busy = disabled || saving || inProgress;
  const dirty =
    mode !== settings.mode ||
    enabled !== settings.enabled ||
    credentialMode !== settings.credentialMode ||
    (credentialMode === "custom" &&
      (card !== (settings.customCampusCardNumber ?? "") || password !== ""));

  const save = async () => {
    if (busy) return;
    setSaving(true);
    setError("");
    setFeedback(undefined);
    setConfirmation(undefined);
    try {
      const next = await onSave({
        mode,
        enabled: mode === "appAuto" && enabled,
        credentialMode,
        campusCardNumber: credentialMode === "custom" ? card || undefined : undefined,
        password: credentialMode === "custom" ? password || undefined : undefined,
      });
      if (mounted.current) {
        setCard(next.customCampusCardNumber ?? "");
        setPassword("");
        toast.success("教工区设置已保存");
      }
    } catch (cause) {
      const msg = String(cause);
      if (mounted.current) setError(msg);
      toast.danger("保存教工区设置失败", { description: msg });
    } finally {
      if (mounted.current) setSaving(false);
    }
  };
  const toggleAuto = async (value: boolean) => {
    setEnabled(value);
    if (value || !settings.enabled) return;
    // Stop immediately without also committing unsaved mode/password drafts.
    setSaving(true);
    setError("");
    setConfirmation(undefined);
    try {
      await onSave({
        mode: settings.mode,
        enabled: false,
        credentialMode: settings.credentialMode,
      });
    } catch (cause) {
      if (mounted.current) setError(String(cause));
    } finally {
      if (mounted.current) setSaving(false);
    }
  };
  const apply = async (token?: string) => {
    if (applyingRef.current || disabled || saving || dirty) return;
    applyingRef.current = true;
    setApplying(true);
    setError("");
    setFeedback(undefined);
    setConfirmation(undefined);
    try {
      const result = await onApply(token);
      if (!mounted.current) return;
      if (result.outcome === "requiresConfirmation" && result.confirmationToken)
        setConfirmation(result);
      else {
        setFeedback(result);
        if (result.outcome === "failed") {
          toast.danger("教工区操作失败", { description: result.message });
        }
      }
    } catch (cause) {
      const msg = String(cause);
      if (mounted.current) setError(msg);
      toast.danger("教工区操作失败", { description: msg });
    } finally {
      applyingRef.current = false;
      if (mounted.current) setApplying(false);
    }
  };

  return (
    <Card className="network-zone" variant="outlined">
      <div slot="headline">教工区登录</div>
      <div slot="subhead">门户地址：net.szu.edu.cn</div>
      <div className="p-4">
        <Tabs
          ref={(host) => nameTablist(host, "教工区登录方式")}
          value={mode}
          onChange={(event) => {
            if (busy) return;
            const key = event.detail.value;
            setMode(key as StaffLoginMode);
            if (key === "macAuth") setEnabled(false);
            setConfirmation(undefined);
          }}
        >
          <Tab value="macAuth" disabled={busy}>
            无感认证（绑定MAC地址）
          </Tab>
          <Tab value="appAuto" disabled={busy}>
            软件自动登录（不绑定MAC地址）
          </Tab>
        </Tabs>
        {mode === "macAuth" ? (
          <p className="field-hint">
            复现门户勾选“记住密码”的登录流程；后续由校园网处理无感认证，无需应用保持后台运行。
          </p>
        ) : (
          <>
            <p className="field-hint">
              不保留门户的“记住密码”Cookie；开启后由应用监测并自动认证，关闭窗口时在后台运行。
            </p>
            <Switch
              aria-label="开启教工区自动登录"
              checked={enabled}
              disabled={disabled || saving}
              onChange={(e: any) => void toggleAuto(e.detail?.checked ?? e.target?.checked)}
            >
              <div className="flex flex-col gap-0.5 ml-2">
                <strong>自动登录</strong>
                <span>新配置手动应用成功后生效，关闭开关立即停止</span>
              </div>
            </Switch>
          </>
        )}
        <Tabs
          ref={(host) => nameTablist(host, "教工区凭据来源")}
          className="network-credential-tabs"
          value={credentialMode}
          onChange={(event) => {
            if (!busy) setCredentialMode(event.detail.value as StaffCredentialMode);
          }}
        >
          <Tab value="currentAccount" disabled={busy}>
            复用已登录账号
          </Tab>
          <Tab value="custom" disabled={busy}>
            单独配置
          </Tab>
        </Tabs>
        {credentialMode === "custom" && (
          <div className="custom-network-form">
            <TextField
              label="校园卡号"
              className="field"
              name="staffCampusCard"
              disabled={busy}
              value={card}
              {...bindField(setCard)}
              placeholder={settings.customConfigured ? "已保存，留空沿用" : "6 位数字"}
            />
            <TextField
              label="密码"
              className="field"
              name="staffPassword"
              disabled={busy}
              type="password"
              value={password}
              {...bindField(setPassword)}
              placeholder={settings.customConfigured ? "已保存，输入新密码可覆盖" : "请输入密码"}
            />
          </div>
        )}
        <p className="field-hint">
          配置状态：{dirty ? "未保存" : settings.needsApply ? "待应用" : "已应用"}；上次已应用模式：
          {staffModeText(settings.appliedMode)}。
        </p>
        <p className="field-hint">
          保存不会主动断网；已在线时，应用模式需要确认下线、解绑本机并重新登录。
        </p>
        <div className="network-action-row">
          <Button variant="filled" disabled={busy} onClick={() => void save()}>
            {saving && <LoadingIndicator size={16} />}保存设置
          </Button>
          <Button variant="outlined" disabled={busy || dirty} onClick={() => void apply()}>
            {inProgress && <LoadingIndicator size={16} />}立即登录
          </Button>
        </div>
        {dirty && <p className="field-hint">请先保存配置，再手动应用。</p>}
        {status.message && (
          <p role="status" className="field-hint">
            {status.message}
          </p>
        )}
        {feedback && (
          <Banner status="default">
            <Banner.Title>结果</Banner.Title>
            <Banner.Description>{feedback.message}</Banner.Description>
          </Banner>
        )}
        {error && (
          <Banner status="danger">
            <Banner.Title>操作失败</Banner.Title>
            <Banner.Description>{error}</Banner.Description>
          </Banner>
        )}
        {confirmation &&
          typeof document !== "undefined" &&
          createPortal(
            <Dialog
              open={!!confirmation}
              headline="确认重新登录教工区？"
              onClose={() => setConfirmation(undefined)}
            >
              <div>{confirmation.message}</div>
              <div slot="actions">
                <Button
                  variant="text"
                  aria-label="取消教工区重登"
                  onClick={() => setConfirmation(undefined)}
                >
                  取消
                </Button>
                <Button
                  variant="filled"
                  disabled={applying}
                  onClick={() => void apply(confirmation.confirmationToken ?? undefined)}
                >
                  确认下线并应用
                </Button>
              </div>
            </Dialog>,
            document.body,
          )}
      </div>
    </Card>
  );
}
