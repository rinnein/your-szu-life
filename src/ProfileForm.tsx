import { useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, LoadingIndicator, TextField, Button } from "material/react";
import { bindField, toast } from "./ui";
import { errorMessage } from "./async-state";
import { usePending } from "./use-pending";
import { useRegisterNextAction, type StepNextAction } from "./step-next-action";
import type { Profile } from "./auth";

export function ProfileForm({
  initial,
  name,
  onlyCampusCard = false,
  onSaved,
  onboarding = false,
  registerNext,
}: {
  initial?: Partial<Profile>;
  name?: string | null;
  onlyCampusCard?: boolean;
  onSaved: () => void | Promise<void>;
  onboarding?: boolean;
  registerNext?: (action: StepNextAction | null) => void;
}) {
  const [studentId, setStudentId] = useState(initial?.studentId ?? "");
  const [campusCardNumber, setCampusCardNumber] = useState(initial?.campusCardNumber ?? "");
  const [displayName, setDisplayName] = useState(name ?? initial?.name ?? "");
  const [password, setPassword] = useState("");
  const pending = usePending();
  const saving = pending.isPending("save");

  const submit = async () => {
    if (!/^\d{10}$/.test(studentId)) {
      toast.danger("学号必须是 10 位数字");
      return;
    }
    if (!/^\d{6}$/.test(campusCardNumber)) {
      toast.danger("校园卡号必须是 6 位数字");
      return;
    }
    if (!onlyCampusCard && !initial?.hasPassword && !password) {
      toast.danger("密码不能为空");
      return;
    }
    // 表单回车与按钮走同一入口；同键登记后重复提交会被直接忽略。
    const outcome = await pending.run("save", async () => {
      await invoke("confirm_profile", {
        input: {
          studentId,
          campusCardNumber,
          name: displayName || undefined,
          password: password || undefined,
        },
      });
    });
    if (outcome.ok) await onSaved();
    else if (!outcome.skipped) {
      toast.danger("保存账户资料失败", { description: errorMessage(outcome.error) });
    }
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void submit();
  };

  useRegisterNextAction(
    registerNext,
    onboarding
      ? {
          label: onlyCampusCard ? "保存并继续" : "保存并进入工作台",
          disabled: saving,
          busy: saving,
          run: submit,
        }
      : null,
  );

  return (
    <Card data-testid="profile-card" variant="outlined">
      <div slot="headline">{onlyCampusCard ? "补充校园卡号" : "账户与个人信息"}</div>
      <div slot="subhead">
        {onlyCampusCard
          ? "已验证的学号和密码已安全保存；请补充校园卡号以完成账户设置。"
          : "学号与校园卡号用于统一认证与校园卡相关服务；密码保存在本机凭据存储中，不会回显。"}
      </div>
      {/* material 的 TextField 自带 label 与 supportingText，不需要外部 Label/Input 组合。 */}
      <form className="settings-form" onSubmit={onSubmit}>
        {!onlyCampusCard && (
          <TextField
            name="name"
            label="姓名"
            value={displayName}
            placeholder="登录后自动获取"
            disabled={saving}
            {...bindField(setDisplayName)}
          />
        )}
        <TextField
          name="studentId"
          label="学号"
          required
          value={studentId}
          disabled={onlyCampusCard || saving}
          {...bindField(setStudentId)}
        />
        <TextField
          name="campusCardNumber"
          label="校园卡号"
          required
          value={campusCardNumber}
          disabled={saving}
          {...bindField(setCampusCardNumber)}
        />
        {!onlyCampusCard && (
          <TextField
            name="password"
            label="密码"
            type="password"
            required={!initial?.hasPassword}
            value={password}
            disabled={saving}
            placeholder={initial?.hasPassword ? "已保存（输入新密码可覆盖）" : "请输入密码"}
            {...bindField(setPassword)}
          />
        )}
        {!onboarding && (
          <div>
            <Button type="submit" variant="filled" disabled={saving}>
              {saving && <LoadingIndicator size={16} />}
              {saving ? "保存中…" : "保存"}
            </Button>
          </div>
        )}
      </form>
    </Card>
  );
}
