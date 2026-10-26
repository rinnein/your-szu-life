import { useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, LoadingIndicator, TextField, Button } from "material/react";
import { toast } from "./ui";
import type { Profile } from "./auth";

export function ProfileForm({
  initial,
  name,
  onlyCampusCard = false,
  onSaved,
}: {
  initial?: Partial<Profile>;
  name?: string | null;
  onlyCampusCard?: boolean;
  onSaved: () => void | Promise<void>;
}) {
  const [studentId, setStudentId] = useState(initial?.studentId ?? "");
  const [campusCardNumber, setCampusCardNumber] = useState(initial?.campusCardNumber ?? "");
  const [displayName, setDisplayName] = useState(name ?? initial?.name ?? "");
  const [password, setPassword] = useState("");
  const [saving, setSaving] = useState(false);

  const save = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
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
    setSaving(true);
    try {
      await invoke("confirm_profile", {
        input: {
          studentId,
          campusCardNumber,
          name: displayName || undefined,
          password: password || undefined,
        },
      });
      await onSaved();
    } catch (error) {
      toast.danger("保存账户资料失败", { description: String(error) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Card className="profile-card" data-testid="profile-card" variant="outlined">
      <div slot="headline">
        <span className="font-semibold text-lg">
          {onlyCampusCard ? "补充校园卡号" : "确认账户资料"}
        </span>
      </div>
      <div slot="subhead">
        {onlyCampusCard
          ? "已验证的学号和密码已安全保存；请补充校园卡号以完成账户设置。"
          : "确认资料后即可进入工作台。"}
      </div>
      <div className="p-4">
        {/* material 的 TextField 自带 label 与 supportingText，不需要外部 Label/Input 组合。 */}
        <form className="profile-form" onSubmit={save}>
          {!onlyCampusCard && (
            <TextField
              name="name"
              label="姓名"
              value={displayName}
              placeholder="登录后自动获取"
              onChange={(e: any) => setDisplayName(e.detail?.value ?? e.target?.value ?? "")}
            />
          )}
          <TextField
            name="studentId"
            label="学号"
            required
            value={studentId}
            disabled={onlyCampusCard}
            onChange={(e: any) => setStudentId(e.detail?.value ?? e.target?.value ?? "")}
          />
          <TextField
            name="campusCardNumber"
            label="校园卡号"
            required
            value={campusCardNumber}
            onChange={(e: any) => setCampusCardNumber(e.detail?.value ?? e.target?.value ?? "")}
          />
          {!onlyCampusCard && (
            <TextField
              name="password"
              label="密码"
              type="password"
              required={!initial?.hasPassword}
              value={password}
              placeholder={initial?.hasPassword ? "已保存（输入新密码可覆盖）" : "请输入密码"}
              onChange={(e: any) => setPassword(e.detail?.value ?? e.target?.value ?? "")}
            />
          )}
          <Button className="w-full" type="submit" variant="filled" disabled={saving}>
            {saving && <LoadingIndicator size={16} />}
            {saving ? "保存中…" : "保存并进入工作台"}
          </Button>
        </form>
      </div>
    </Card>
  );
}
