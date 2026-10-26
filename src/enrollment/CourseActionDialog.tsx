import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { Dialog, TextField, Button } from "material/react";
import { useEnrollment } from "./use-enrollment";
import { OperationError } from "./shared";
import { loadPreferences } from "./preferences";
import { gears, type Course, type Gear } from "./model";
export default function CourseActionDialog({
  selection,
  onClose,
}: {
  selection: { course: Course; drop?: boolean } | undefined;
  onClose: () => void;
}) {
  const { snapshot: s, run, busy, errors } = useEnrollment();
  const dialog = selection;
  const setDialog = () => onClose();
  const [oldId, setOldId] = useState("");
  const [threshold, setThreshold] = useState("2");
  const [group, setGroup] = useState("");
  const [priority, setPriority] = useState("0");
  const [gear, setGear] = useState<Gear>("medium");
  const [start, setStart] = useState("");
  const [labs, setLabs] = useState<Course[][]>([]);
  const [labIds, setLabIds] = useState<Record<number, string>>({});
  useEffect(() => {
    if (!selection) return;
    const defaults = loadPreferences();
    setGear(defaults.gear);
    setStart(defaults.startAt);
    setOldId("");
    setThreshold("2");
    setGroup("");
    setPriority("0");
    setLabs([]);
    setLabIds({});
    if (!selection.drop) void run({ type: "selected" });
    if (selection.course.hasTest && !selection.drop)
      void run<Course[][]>({ type: "labs", course: selection.course }).then((v) => v && setLabs(v));
  }, [selection, run]);
  if (!dialog || typeof document === "undefined") return null;

  return createPortal(
    <Dialog
      open={!!dialog}
      headline={`${dialog?.drop ? "确认退课" : "配置选课任务"} · ${dialog?.course.name}`}
      onClose={() => setDialog()}
    >
      <div className="max-h-[75vh] overflow-y-auto">
        <OperationError message={errors.add || errors.drop || errors.labs || errors.selected} />
        {dialog?.drop ? (
          <p>退课后名额可能被其他人选走。</p>
        ) : (
          <div className="grid gap-3">
            <TextField
              label="备选组（留空为独立目标）"
              value={group}
              onChange={(e: any) => setGroup(e.detail?.value ?? e.target?.value ?? "")}
            />
            <TextField
              label="执行优先级"
              value={priority}
              onChange={(e: any) => setPriority(e.detail?.value ?? e.target?.value ?? "")}
              type="number"
            />
            <TextField
              label="预约时间（留空立即）"
              value={start}
              onChange={(e: any) => setStart(e.detail?.value ?? e.target?.value ?? "")}
              type="datetime-local"
            />
            <div className="flex flex-wrap gap-2">
              {(Object.keys(gears) as Gear[]).map((g) => (
                <Button
                  key={g}
                  variant={g === gear ? "filled" : "tonal"}
                  onClick={() => setGear(g)}
                >
                  {gears[g]}
                </Button>
              ))}
            </div>
            <p>自动换课：选择要退的旧课，留空为普通抢课。</p>
            <Button variant="tonal" onClick={() => setOldId("")}>
              不退旧课
            </Button>
            {s?.selected
              .filter((c) => c.id !== dialog?.course.id)
              .map((c) => (
                <Button
                  key={c.id}
                  variant={c.id === oldId ? "filled" : "tonal"}
                  onClick={() => setOldId(c.id)}
                >
                  {c.name}
                </Button>
              ))}
            {oldId && (
              <>
                <TextField
                  label="新课余量触发阈值"
                  value={threshold}
                  onChange={(e: any) => setThreshold(e.detail?.value ?? e.target?.value ?? "")}
                  type="number"
                />
                <p>确认后授权循环退旧抢新；新课满员会以 ultra 回抢旧课，但不保证能恢复原名额。</p>
              </>
            )}
            {dialog?.course.hasTest && (
              <div>
                <p>每组选择一个关联实验班</p>
                {labs.map((group, i) => (
                  <div key={i}>
                    {group.map((lab) => (
                      <Button
                        key={lab.id}
                        variant={labIds[i] === lab.id ? "filled" : "tonal"}
                        onClick={() => setLabIds({ ...labIds, [i]: lab.id })}
                      >
                        {lab.teacher} · {lab.schedule}
                      </Button>
                    ))}
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
      <div slot="actions" className="flex items-center justify-end gap-2 w-full">
        <Button variant="tonal" onClick={() => setDialog()}>
          取消
        </Button>
        {!dialog?.drop && !oldId && (
          <Button
            disabled={
              busy ||
              !!(dialog?.course.hasTest && (!labs.length || labs.some((_, i) => !labIds[i])))
            }
            onClick={() => {
              if (!dialog) return;
              void run({
                type: "add",
                input: {
                  course: { ...dialog.course, testIds: Object.values(labIds) },
                  oldCourse: null,
                  threshold: 2,
                  group: null,
                  priority: 0,
                  gear: "medium",
                  startAt: null,
                  riskConfirmed: false,
                  oneShot: true,
                },
              }).then((v) => v && setDialog());
            }}
          >
            立即选课一次
          </Button>
        )}
        <Button
          disabled={
            busy ||
            !!(
              dialog?.course.hasTest &&
              (!labs.length || labs.some((_, i) => !labIds[i])) &&
              !dialog.drop
            )
          }
          onClick={() => {
            if (!dialog) return;
            const action = dialog.drop
              ? { type: "drop" as const, course: dialog.course }
              : {
                  type: "add" as const,
                  input: {
                    course: { ...dialog.course, testIds: Object.values(labIds) },
                    oldCourse: s?.selected.find((c) => c.id === oldId) ?? null,
                    threshold: Number(threshold),
                    group: group || null,
                    priority: Number(priority),
                    gear,
                    startAt: start ? new Date(start).getTime() : null,
                    riskConfirmed: !!oldId,
                  },
                };
            void run(action).then((v) => v && setDialog());
          }}
        >
          确认{dialog?.drop ? "退课" : oldId ? "授权换课" : "加入任务"}
        </Button>
      </div>
    </Dialog>,
    document.body,
  );
}
