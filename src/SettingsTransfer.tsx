import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, TextField, Button } from "material/react";
import { Banner, toast } from "./ui";
import { LuCheck, LuCopy, LuDownload, LuKeyRound, LuUpload } from "react-icons/lu";
import { copyText } from "./clipboard";

type Preview = { conflicts: string[]; incoming: number };

const conflictNames: Record<string, string> = {
  "fixed.account.profile": "账户资料和密码",
  "runtime.auth.session": "校园统一认证会话",
  "fixed.uooc.ai": "UOOC AI 连接配置",
  "runtime.uooc.session": "UOOC 登录会话",
  "fixed.oj.accounts": "OJ 账户设置",
  "runtime.oj.sessions": "OJ 登录会话",
  "runtime.academic.sessions": "选课登录会话",
  "fixed.network.dorm.settings": "宿舍区校园网设置",
  "fixed.network.dorm.credentials": "宿舍区校园网凭据",
  "fixed.network.staff.settings": "教工区校园网设置",
  "fixed.network.staff.credentials": "教工区校园网凭据",
  "fixed.electricity.location": "宿舍电费位置",
  "runtime.electricity.monitor": "电费监控与提醒记录",
  "runtime.account.pending": "待确认的账户资料",
};

function errorMessage(error: unknown, fallback: string) {
  return typeof error === "string" ? error : error instanceof Error ? error.message : fallback;
}

export default function SettingsTransfer() {
  const [exportPassword, setExportPassword] = useState("");
  const [importPassword, setImportPassword] = useState("");
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<Preview>();
  const [exported, setExported] = useState("");
  const [copied, setCopied] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const clearFeedback = () => {
    setMessage("");
    setError("");
  };

  const exportSettings = async () => {
    setBusy(true);
    clearFeedback();
    setCopied(false);
    try {
      const value = await invoke<string>("export_settings", { password: exportPassword });
      setExported(value);
      setExportPassword("");
      setMessage("加密字符串已生成。请与一次性密码分开保存并传送。");
      toast.success("加密字符串已生成");
    } catch (e) {
      const err = errorMessage(e, "设置导出失败");
      setError(err);
      toast.danger(err);
    } finally {
      setBusy(false);
    }
  };

  const inspectImport = async () => {
    setBusy(true);
    clearFeedback();
    try {
      setPreview(
        await invoke<Preview>("preview_import_settings", { text, password: importPassword }),
      );
    } catch (e) {
      setPreview(undefined);
      const err = errorMessage(e, "无法读取加密字符串");
      setError(err);
      toast.danger(err);
    } finally {
      setBusy(false);
    }
  };

  const importSettings = async (overwrite: boolean) => {
    setBusy(true);
    clearFeedback();
    try {
      const result = await invoke<Preview>("import_settings", {
        text,
        password: importPassword,
        options: { overwrite },
      });
      setPreview(result);
      setImportPassword("");
      const msg = overwrite
        ? "设置已导入，冲突项已由导入内容覆盖。"
        : "设置已导入，冲突项保留本机内容。";
      setMessage(msg);
      toast.success(msg);
    } catch (e) {
      const err = errorMessage(e, "设置导入失败");
      setError(err);
      toast.danger(err);
    } finally {
      setBusy(false);
    }
  };

  const updateText = (value: string) => {
    setText(value);
    setPreview(undefined);
    setExported("");
    setCopied(false);
    clearFeedback();
  };

  return (
    <section aria-labelledby="settings-transfer-title" className="grid gap-4">
      <header className="max-w-2xl">
        <h2 id="settings-transfer-title" className="text-xl font-semibold tracking-tight">
          设置迁移
        </h2>
        <p className="mt-1.5 text-sm leading-relaxed text-[var(--szu-muted)]">
          用一次性密码保护设置包。密码不会保存，建议通过不同渠道传送密码和加密字符串。
        </p>
      </header>

      {(error || message) && (
        <Banner status={error ? "danger" : "success"}>
          <Banner.Title>{error ? "操作未完成" : "操作完成"}</Banner.Title>
          <Banner.Description>{error || message}</Banner.Description>
        </Banner>
      )}

      <div className="grid items-start gap-4 lg:grid-cols-2">
        <Card
          variant="outlined"
          className="h-full border border-[rgba(37,40,88,0.1)] shadow-[0_12px_35px_#2528580d]"
        >
          <div slot="headline" className="flex items-center gap-3">
            <div
              className="flex size-10 items-center justify-center rounded-xl bg-[var(--szu-brand-soft)] text-[var(--szu-brand)]"
              aria-hidden="true"
            >
              <LuDownload className="size-5" />
            </div>
            <div className="grid gap-1">
              <span className="font-semibold">加密导出</span>
            </div>
          </div>
          <div slot="subhead">生成可复制的加密字符串，在另一台设备上导入。</div>
          <div className="grid gap-4 p-4">
            <TextField
              type="password"
              label="设置一次性密码"
              value={exportPassword}
              placeholder="至少 8 个字符"
              autocomplete="new-password"
              disabled={busy}
              onChange={(e: any) => setExportPassword(e.detail?.value ?? e.target?.value ?? "")}
            />
            <Button
              className="w-fit"
              onClick={() => void exportSettings()}
              disabled={busy || exportPassword.length < 8}
            >
              <LuKeyRound aria-hidden="true" />
              {busy ? "正在加密…" : "生成加密字符串"}
            </Button>
            {exported && (
              <div className="grid gap-2">
                <div className="flex items-center justify-between gap-3">
                  <span className="field-label">加密字符串</span>
                  <Button
                    size="xs"
                    variant="tonal"
                    onClick={() =>
                      void copyText(exported).then((success) => {
                        setCopied(success);
                        setError(success ? "" : "无法自动复制，请手动选择并复制加密字符串。");
                      })
                    }
                  >
                    {copied ? <LuCheck aria-hidden="true" /> : <LuCopy aria-hidden="true" />}
                    {copied ? "已复制" : "复制"}
                  </Button>
                </div>
                <TextField
                  type="multiline"
                  label="导出的加密设置字符串"
                  className="export-result"
                  value={exported}
                  readonly
                />
              </div>
            )}
          </div>
        </Card>

        <Card
          variant="outlined"
          className="h-full border border-[rgba(37,40,88,0.1)] shadow-[0_12px_35px_#2528580d]"
        >
          <div slot="headline" className="flex items-center gap-3">
            <div
              className="flex size-10 items-center justify-center rounded-xl bg-[var(--szu-brand-soft)] text-[var(--szu-brand)]"
              aria-hidden="true"
            >
              <LuUpload className="size-5" />
            </div>
            <div className="grid gap-1">
              <span className="font-semibold">导入到此设备</span>
            </div>
          </div>
          <div slot="subhead">检查现有设置冲突，再选择保留本机或覆盖。</div>
          <div className="grid gap-4 p-4">
            <TextField
              type="multiline"
              label="加密字符串"
              value={text}
              placeholder="粘贴另一台设备生成的加密字符串"
              disabled={busy}
              onChange={(e: any) => updateText(e.detail?.value ?? e.target?.value ?? "")}
            />
            <TextField
              type="password"
              label="解密用的一次性密码"
              value={importPassword}
              placeholder="输入导出时设置的密码"
              autocomplete="new-password"
              disabled={busy}
              onChange={(e: any) => {
                setImportPassword(e.detail?.value ?? e.target?.value ?? "");
                setPreview(undefined);
                clearFeedback();
              }}
            />
            {!preview ? (
              <Button
                className="w-fit"
                variant="tonal"
                onClick={() => void inspectImport()}
                disabled={busy || !text.trim() || importPassword.length < 8}
              >
                检查设置包
              </Button>
            ) : (
              <div className="grid gap-3 rounded-xl bg-[var(--szu-brand-soft)] p-4">
                <div>
                  <p className="font-medium">找到 {preview.incoming} 项设置</p>
                  <p className="mt-1 text-sm text-[var(--szu-muted)]">
                    {preview.conflicts.length === 0
                      ? "与本机设置没有冲突，可以直接导入。"
                      : `${preview.conflicts.length} 项与本机内容重叠。选择如何处理这些项目。`}
                  </p>
                </div>
                {preview.conflicts.length > 0 && (
                  <div className="flex flex-wrap gap-2" aria-label="冲突设置项目">
                    {preview.conflicts.map((key) => (
                      <span
                        key={key}
                        className="rounded-lg border border-[rgba(37,40,88,0.12)] bg-[var(--szu-surface)] px-2.5 py-1 text-xs text-[var(--szu-ink)]"
                      >
                        {conflictNames[key] ?? "其他设置"}
                      </span>
                    ))}
                  </div>
                )}
                <div className="flex flex-wrap gap-2">
                  <Button onClick={() => void importSettings(false)} disabled={busy}>
                    {preview.conflicts.length > 0 ? "导入并保留本机内容" : "导入设置"}
                  </Button>
                  {preview.conflicts.length > 0 && (
                    <Button
                      variant="tonal"
                      onClick={() => void importSettings(true)}
                      disabled={busy}
                    >
                      覆盖冲突项
                    </Button>
                  )}
                </div>
              </div>
            )}
          </div>
        </Card>
      </div>
    </section>
  );
}
