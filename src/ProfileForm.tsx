import { useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Card, Form, Input, Label, Spinner, TextField, toast } from "@heroui/react";
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
    } catch (err) {
      toast.danger("保存账户资料失败", { description: String(err) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Card className="profile-card" variant="default" data-testid="profile-card">
      <Card.Header>
        <Card.Title>{onlyCampusCard ? "补充校园卡号" : "确认账户资料"}</Card.Title>
        <Card.Description>
          {onlyCampusCard
            ? "已验证的学号和密码已安全保存；请补充校园卡号以完成账户设置。"
            : "确认资料后即可进入工作台。"}
        </Card.Description>
      </Card.Header>
      <Card.Content>
        <Form onSubmit={save} validationBehavior="aria">
          {!onlyCampusCard && (
            <TextField className="field" name="name">
              <Label>姓名</Label>
              <Input
                value={displayName}
                onChange={(event) => setDisplayName(event.target.value)}
                placeholder="登录后自动获取"
              />
            </TextField>
          )}
          <TextField className="field" name="studentId" isRequired>
            <Label>学号</Label>
            <Input
              inputMode="numeric"
              value={studentId}
              disabled={onlyCampusCard}
              onChange={(event) => setStudentId(event.target.value)}
            />
          </TextField>
          <TextField className="field" name="campusCardNumber" isRequired>
            <Label>校园卡号</Label>
            <Input
              inputMode="numeric"
              value={campusCardNumber}
              autoFocus={onlyCampusCard}
              onChange={(event) => setCampusCardNumber(event.target.value)}
            />
          </TextField>
          {!onlyCampusCard && (
            <TextField className="field" name="password" isRequired={!initial?.hasPassword}>
              <Label>密码</Label>
              <Input
                type="password"
                placeholder={initial?.hasPassword ? "已保存（输入新密码可覆盖）" : "请输入密码"}
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
            </TextField>
          )}
          <Button fullWidth type="submit" variant="primary" isDisabled={saving}>
            {saving && <Spinner size="sm" color="current" />}
            {saving ? "保存中…" : "保存并进入工作台"}
          </Button>
        </Form>
      </Card.Content>
    </Card>
  );
}
