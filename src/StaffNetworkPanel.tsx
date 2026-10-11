import { useEffect, useRef, useState } from "react";
import {
  Alert,
  AlertDialog,
  Button,
  Card,
  Input,
  Label,
  Spinner,
  Switch,
  Tabs,
  TextField,
} from "@heroui/react";
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
      }
    } catch (cause) {
      if (mounted.current) setError(String(cause));
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
      else setFeedback(result);
    } catch (cause) {
      if (mounted.current) setError(String(cause));
    } finally {
      applyingRef.current = false;
      if (mounted.current) setApplying(false);
    }
  };

  return (
    <Card className="network-zone" variant="default">
      <Card.Header>
        <Card.Title>教工区登录</Card.Title>
        <Card.Description>门户地址：net.szu.edu.cn</Card.Description>
      </Card.Header>
      <Card.Content>
        <Tabs
          selectedKey={mode}
          onSelectionChange={(key) => {
            if (busy) return;
            setMode(String(key) as StaffLoginMode);
            if (key === "macAuth") setEnabled(false);
            setConfirmation(undefined);
          }}
        >
          <Tabs.List aria-label="教工区登录方式">
            <Tabs.Tab id="macAuth" isDisabled={busy}>
              无感认证（绑定MAC地址）
            </Tabs.Tab>
            <Tabs.Tab id="appAuto" isDisabled={busy}>
              软件自动登录（不绑定MAC地址）
            </Tabs.Tab>
          </Tabs.List>
          <Tabs.Panel id="macAuth">
            <p className="field-hint">
              复现门户勾选“记住密码”的登录流程；后续由校园网处理无感认证，无需应用保持后台运行。
            </p>
          </Tabs.Panel>
          <Tabs.Panel id="appAuto">
            <p className="field-hint">
              不保留门户的“记住密码”Cookie；开启后由应用监测并自动认证，关闭窗口时在后台运行。
            </p>
            <Switch
              aria-label="开启教工区自动登录"
              isSelected={enabled}
              isDisabled={disabled || saving}
              onChange={(value) => void toggleAuto(value)}
            >
              <Switch.Content>
                <strong>自动登录</strong>
                <span>新配置手动应用成功后生效，关闭开关立即停止</span>
              </Switch.Content>
              <Switch.Control>
                <Switch.Thumb />
              </Switch.Control>
            </Switch>
          </Tabs.Panel>
        </Tabs>
        <Tabs
          className="network-credential-tabs"
          selectedKey={credentialMode}
          onSelectionChange={(key) => {
            if (!busy) setCredentialMode(String(key) as StaffCredentialMode);
          }}
        >
          <Tabs.List aria-label="教工区凭据来源">
            <Tabs.Tab id="currentAccount" isDisabled={busy}>
              复用已登录账号
            </Tabs.Tab>
            <Tabs.Tab id="custom" isDisabled={busy}>
              单独配置
            </Tabs.Tab>
          </Tabs.List>
          <Tabs.Panel id="custom">
            <div className="custom-network-form">
              <TextField className="field" name="staffCampusCard" isDisabled={busy}>
                <Label>校园卡号</Label>
                <Input
                  inputMode="numeric"
                  value={card}
                  onChange={(event) => setCard(event.target.value)}
                  placeholder={settings.customConfigured ? "已保存，留空沿用" : "6 位数字"}
                />
              </TextField>
              <TextField className="field" name="staffPassword" isDisabled={busy}>
                <Label>密码</Label>
                <Input
                  type="password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                  placeholder={
                    settings.customConfigured ? "已保存，输入新密码可覆盖" : "请输入密码"
                  }
                />
              </TextField>
            </div>
          </Tabs.Panel>
        </Tabs>
        <p className="field-hint">
          配置状态：{dirty ? "未保存" : settings.needsApply ? "待应用" : "已应用"}；上次已应用模式：
          {staffModeText(settings.appliedMode)}。
        </p>
        <p className="field-hint">
          保存不会主动断网；已在线时，应用模式需要确认下线、解绑本机并重新登录。
        </p>
        <div className="network-action-row">
          <Button variant="primary" isDisabled={busy} onPress={() => void save()}>
            {saving && <Spinner size="sm" color="current" />}保存设置
          </Button>
          <Button variant="outline" isDisabled={busy || dirty} onPress={() => void apply()}>
            {inProgress && <Spinner size="sm" color="current" />}立即登录
          </Button>
        </div>
        {dirty && <p className="field-hint">请先保存配置，再手动应用。</p>}
        {status.message && (
          <p role="status" className="field-hint">
            {status.message}
          </p>
        )}
        {feedback && (
          <Alert
            status={
              feedback.outcome === "applied" || feedback.outcome === "alreadyOnline"
                ? "success"
                : feedback.outcome === "unknown"
                  ? "warning"
                  : "danger"
            }
          >
            <Alert.Content>
              <Alert.Title>结果</Alert.Title>
              <Alert.Description>{feedback.message}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {error && (
          <Alert status="danger">
            <Alert.Content>
              <Alert.Title>操作失败</Alert.Title>
              <Alert.Description>{error}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        <AlertDialog
          isOpen={!!confirmation}
          onOpenChange={(open) => {
            if (!open) setConfirmation(undefined);
          }}
        >
          <AlertDialog.Backdrop>
            <AlertDialog.Container size="sm">
              <AlertDialog.Dialog>
                <AlertDialog.Icon status="warning" />
                <AlertDialog.Header>
                  <AlertDialog.Heading>确认重新登录教工区？</AlertDialog.Heading>
                </AlertDialog.Header>
                <AlertDialog.Body>{confirmation?.message}</AlertDialog.Body>
                <AlertDialog.Footer>
                  <AlertDialog.CloseTrigger aria-label="取消教工区重登" className="dialog-cancel" />
                  <Button
                    variant="danger"
                    isDisabled={applying}
                    onPress={() => void apply(confirmation?.confirmationToken ?? undefined)}
                  >
                    确认下线并应用
                  </Button>
                </AlertDialog.Footer>
              </AlertDialog.Dialog>
            </AlertDialog.Container>
          </AlertDialog.Backdrop>
        </AlertDialog>
      </Card.Content>
    </Card>
  );
}
