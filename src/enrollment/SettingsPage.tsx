import { useEffect, useState } from "react";
import { Button, Card, Input, Label, TextField } from "@heroui/react";
import { useLogicalNavigation } from "../logical-navigation";
import { useCacheMode } from "./view-mode";
import { useEnrollment } from "./use-enrollment";
import AccessModes from "./AccessModes";
import { Choice, OperationError } from "./shared";
import { loadPreferences, savePreferences } from "./preferences";
import { gears, type Gear } from "./model";
type Options = {
  batches: { code: string; name: string; typeName: string }[];
  campuses: { code: string; name: string }[];
};
export default function SettingsPage() {
  const cacheOnly = useCacheMode();
  const { snapshot: s, run, errors, pending } = useEnrollment(cacheOnly);
  const nav = useLogicalNavigation();
  const [prefs, setPrefs] = useState(loadPreferences);
  const [message, setMessage] = useState("");
  const [options, setOptions] = useState<Options>();
  useEffect(() => {
    if (cacheOnly) return;
    void run<Options>({ type: "options" }).then((v) => v && setOptions(v));
  }, [run, cacheOnly]);
  if (cacheOnly)
    return (
      <Card>
        <Card.Content>当前为只读缓存模式。退出缓存模式后可修改任务和访问配置。</Card.Content>
      </Card>
    );
  return (
    <div className="grid gap-5">
      <Card>
        <Card.Header>
          <Card.Title>新任务默认配置</Card.Title>
          <Card.Description>仅用于之后创建的任务，不改变正在运行的课程。</Card.Description>
        </Card.Header>
        <Card.Content className="grid gap-4 sm:grid-cols-2">
          <Choice
            label="默认挡位"
            value={prefs.gear}
            onChange={(gear) => setPrefs((p) => ({ ...p, gear: gear as Gear }))}
            items={Object.entries(gears).map(([id, label]) => ({ id, label }))}
          />
          <TextField
            value={prefs.startAt}
            onChange={(startAt) => setPrefs((p) => ({ ...p, startAt }))}
          >
            <Label>默认预约时间</Label>
            <Input type="datetime-local" />
          </TextField>
        </Card.Content>
        <Card.Footer className="gap-3">
          <Button
            onPress={() => {
              try {
                savePreferences(prefs);
                setMessage("已保存");
              } catch {
                setMessage("保存失败，请检查本地存储");
              }
            }}
          >
            保存配置
          </Button>
          <span role="status" className="text-sm text-muted">
            {message}
          </span>
        </Card.Footer>
      </Card>
      <Card>
        <Card.Header>
          <Card.Title>批次与校区</Card.Title>
          <Card.Description>切换后保留各自缓存，运行中的任务会暂停。</Card.Description>
        </Card.Header>
        <Card.Content className="grid gap-3 sm:grid-cols-2">
          {options && (
            <>
              <Choice
                label="选课批次"
                value={s?.context.batch ?? ""}
                disabled={!!pending.context}
                items={options.batches.map((b) => ({ id: b.code, label: b.name || b.typeName }))}
                onChange={(batch) =>
                  void run({
                    type: "context",
                    batch,
                    campus: s?.context.campus || options.campuses[0]?.code,
                  })
                }
              />
              <Choice
                label="校区"
                value={s?.context.campus ?? ""}
                disabled={!!pending.context}
                items={options.campuses.map((c) => ({ id: c.code, label: c.name }))}
                onChange={(campus) =>
                  void run({
                    type: "context",
                    campus,
                    batch: s?.context.batch || options.batches[0]?.code,
                  })
                }
              />
            </>
          )}
          <OperationError message={errors.options || errors.context} />
        </Card.Content>
      </Card>
      <Card>
        <Card.Header>
          <Card.Title>访问与认证</Card.Title>
          <Card.Description>切换访问方式会暂停提交；存在未决请求时须先核验。</Card.Description>
        </Card.Header>
        <Card.Content>
          <AccessModes />
        </Card.Content>
        <Card.Footer>
          <Button
            variant="secondary"
            onPress={() => void nav.replace("/enrollment/login?returnTo=%2Fenrollment%2Fsettings")}
          >
            重新验证选课登录
          </Button>
        </Card.Footer>
      </Card>
    </div>
  );
}
