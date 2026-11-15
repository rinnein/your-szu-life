import { useEffect, useState } from "react";
import { Button, Card, Checkbox, Switch, TextField } from "material/react";
import { Banner, NumberField, bindField, toast } from "../../ui";
import { LoadingIndicator } from "material/react";
import type { UoocSnapshot, UoocSettings } from "../state/types";
import { AiSettings } from "./AiSettings";
import { useLogicalNavigation } from "../../logical-navigation";

const DEFAULTS: UoocSettings = {
  concurrency: 3,
  reportIntervalSec: 20,
  speedMultiplier: 2,
  dryRun: false,
  discussionEnabled: true,
  discussionContent: "要好好学习",
  discussionIntervalMin: 30,
};

interface SettingsTabProps {
  snapshot: UoocSnapshot;
  ready: boolean;
  action: (key: string, command: string, args?: Record<string, unknown>) => Promise<boolean>;
  onMode: (mode: UoocSettings extends never ? never : "supervised" | "approval" | "yolo") => void;
}

/** 任务运行中会被读取、需要先暂停的四个字段。 */
const RUNTIME_KEYS = ["concurrency", "reportIntervalSec", "speedMultiplier", "dryRun"] as const;
type RuntimeKey = (typeof RUNTIME_KEYS)[number];

/**
 * 运行设置 tab：开关与讨论字段即时保存（后端在任务运行中也接受），
 * 四个运行字段在任务运行中锁定并说明原因。
 */
export function SettingsTab({ snapshot, ready, action, onMode }: SettingsTabProps) {
  const navigation = useLogicalNavigation();
  const saved = snapshot.settings;
  const [draft, setDraft] = useState<UoocSettings>(saved);
  const [savingKey, setSavingKey] = useState<string>();

  useEffect(() => {
    setDraft({ ...DEFAULTS, ...saved });
  }, [saved]);

  const taskRunning = snapshot.tasks.some(
    (task) => task.status === "running" || task.status === "queued",
  );

  /** 保存一个字段：失败时 toast 并回滚该字段。 */
  const commit = async (key: keyof UoocSettings, value: unknown) => {
    const next = { ...draft, [key]: value } as UoocSettings;
    setDraft(next);
    const saving = `settings:${String(key)}`;
    setSavingKey(saving);
    const ok = await action(saving, "save_uooc_settings", { settings: next });
    setSavingKey(undefined);
    if (!ok) setDraft({ ...DEFAULTS, ...saved });
    return ok;
  };

  const discussionEnabled = draft.discussionEnabled ?? true;

  return (
    <div className="grid gap-4">
      <Card variant="outlined">
        <div slot="headline">运行设置</div>
        <div slot="subhead">开关与讨论字段即时生效；所有课程共享并发额度。</div>
        <div className="grid gap-5 p-4">
          {taskRunning && (
            <Banner status="warning">
              <Banner.Title>部分字段已锁定</Banner.Title>
              <Banner.Description>
                并发数、上报间隔、目标倍率与仅预览会被正在运行的任务读取；暂停全部任务后才能修改。
              </Banner.Description>
            </Banner>
          )}
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {(
              [
                { key: "concurrency", label: "视频并发数", min: 1, step: 1, max: undefined },
                { key: "reportIntervalSec", label: "上报间隔（秒）", min: 1, step: 1, max: undefined },
                { key: "speedMultiplier", label: "目标倍率", min: 0.1, step: 0.1, max: undefined },
                {
                  key: "discussionIntervalMin",
                  label: "讨论间隔（分钟）",
                  min: 1,
                  step: 1,
                  max: 1440,
                },
              ] as const
            ).map(({ key, label, min, step, max }) => (
              <NumberField
                key={key}
                label={label}
                minValue={min}
                maxValue={max}
                step={step}
                value={(draft[key] as number) ?? 30}
                disabled={
                  (RUNTIME_KEYS.includes(key as RuntimeKey) && taskRunning) ||
                  savingKey === `settings:${key}` ||
                  !ready
                }
                onCommit={(value) => void commit(key, value)}
              />
            ))}
          </div>
          <Checkbox
            checked={draft.dryRun}
            disabled={taskRunning || savingKey === "settings:dryRun" || !ready}
            onChange={(e: unknown) => {
              const checked = Boolean(
                (e as { detail?: { checked?: boolean } })?.detail?.checked ??
                  (e as { target?: { checked?: boolean } })?.target?.checked,
              );
              void commit("dryRun", checked);
            }}
          >
            仅预览（dry-run），不提交视频进度
          </Checkbox>
          <p className="text-sm text-muted">
            视频时长成功获取后会保存在本机，后续任务和重启后均直接复用。仅处理视频，测验、作业与考试需自行完成。
          </p>
          <div className="grid gap-3">
            <Switch
              checked={discussionEnabled}
              disabled={savingKey === "settings:discussionEnabled" || !ready}
              onChange={(e: unknown) => {
                const checked = Boolean(
                  (e as { detail?: { checked?: boolean } })?.detail?.checked ??
                    (e as { target?: { checked?: boolean } })?.target?.checked,
                );
                void commit("discussionEnabled", checked);
              }}
            >
              <span className="ml-2 flex flex-col gap-0.5">
                <span className="text-sm font-medium">自动完成讨论</span>
                <span className="text-xs text-muted">
                  仅处理有讨论权重且完成率未达 100% 的课程，课程之间至少间隔 {draft.discussionIntervalMin ?? 30} 分钟。
                </span>
              </span>
            </Switch>
            <div className="grid gap-2">
              <TextField
                type="multiline"
                label="无评论时的发帖内容"
                value={draft.discussionContent ?? DEFAULTS.discussionContent}
                maxlength={500}
                disabled={!discussionEnabled || !ready}
                {...bindField({
                  input: (value) => setDraft((old) => ({ ...old, discussionContent: value })),
                  change: (value) => void commit("discussionContent", value),
                })}
                className="w-full"
              />
              <span className="text-xs text-muted">
                发送时会自动包裹为一段 HTML，主题固定为“要好好学习”。失焦或回车后保存。
              </span>
            </div>
          </div>
        </div>
      </Card>
      <AiSettings
        onMode={onMode}
        onOpenAiSettings={() => void navigation.push("/settings?tab=ai")}
      />
      {savingKey && (
        <p role="status" className="flex items-center gap-2 text-sm text-muted">
          <LoadingIndicator size={16} aria-label="正在保存" />
          正在保存…
        </p>
      )}
    </div>
  );
}

export { toast, Button };
