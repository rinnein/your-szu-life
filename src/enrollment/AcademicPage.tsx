import { useCallback, useEffect, useMemo, useState } from "react";
import { Button, Card, Chip, Spinner } from "@heroui/react";
import { LuRefreshCw } from "react-icons/lu";
import { PageShell } from "../components";
import { useEnrollment } from "./use-enrollment";
import { ModuleHeader, OperationError } from "./shared";
import AcademicGraph from "./academic/AcademicGraph";
import CourseQueryModal from "./academic/CourseQueryModal";
import { buildTree, creditSummary } from "./academic/tree";
import { getViewState, saveViewState, migrateViewState } from "./academic/view-state";
export default function AcademicPage() {
  const { snapshot: s, run, portal, pending, errors } = useEnrollment();
  const [calculating, setCalculating] = useState(false);
  const [progress, setProgress] = useState("");
  const viewKey = `${s?.context.student ?? "pending"}:${s?.academic?.summary[0]?.PYFADM ?? "plan"}`;
  const [selected, setSelected] = useState<string>();
  useEffect(() => {
    setSelected(getViewState(viewKey).selected);
  }, [viewKey]);
  const tree = useMemo(() => (s?.academic ? buildTree(s.academic) : undefined), [s?.academic]);
  if (tree) migrateViewState(viewKey, tree);
  useEffect(() => {
    if (tree) setSelected(migrateViewState(viewKey, tree).selected);
  }, [tree, viewKey]);
  const open = useCallback(
    (id: string) => {
      setSelected(id);
      saveViewState(viewKey, { selected: id });
    },
    [viewKey],
  );
  const close = () => {
    setSelected(undefined);
    saveViewState(viewKey, { selected: undefined });
  };
  useEffect(() => {
    if (!calculating) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout>;
    const deadline = Date.now() + 180000;
    const poll = async () => {
      const v = await run<{ datas?: { byscjd?: { rows?: { ZS: number; YWCS: number }[] } } }>({
        type: "academicProgress",
      });
      if (!live) return;
      const row = v?.datas?.byscjd?.rows?.[0];
      if (row) {
        setProgress(`${row.YWCS}/${row.ZS}`);
        if (Number(row.ZS) > 0 && Number(row.YWCS) >= Number(row.ZS)) {
          setCalculating(false);
          void run({ type: "academic", recalculate: false });
          return;
        }
      }
      if (Date.now() > deadline) {
        setCalculating(false);
        setProgress("计算仍未完成，请稍后查询进度");
        return;
      }
      timer = setTimeout(() => void poll(), 2000);
    };
    void poll();
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [calculating, run]);
  return (
    <PageShell>
      <div className="mx-auto grid w-full max-w-[1600px] min-w-0 gap-5">
        <ModuleHeader
          title="学业完成查询"
          description="展开培养方案，点击任意节点检索课程"
          actions={
            <>
              <Button
                size="sm"
                variant="secondary"
                isPending={!!pending["portal-academic"]}
                onPress={() => void portal("academic")}
              >
                官方认证
              </Button>
              <Button
                size="sm"
                variant="secondary"
                isPending={!!pending.academic && !calculating}
                onPress={() => void run({ type: "academic", recalculate: false })}
              >
                <LuRefreshCw />
                刷新
              </Button>
              <Button
                size="sm"
                variant="tertiary"
                isDisabled={calculating || !!pending.academic}
                onPress={() =>
                  void run({ type: "academic", recalculate: true }).then(
                    (v) => v && setCalculating(true),
                  )
                }
              >
                重新计算
              </Button>
            </>
          }
        />
        <OperationError
          message={
            errors.snapshot ||
            errors.academic ||
            errors.academicProgress ||
            errors["portal-academic"]
          }
        />
        {s?.academic && (
          <div className="flex flex-wrap items-center justify-between gap-3 text-sm">
            <Chip variant="soft">{creditSummary(s.academic.summary[0] ?? {})}</Chip>
            <span className="text-muted">
              缓存更新于 {new Date(s.academic.updatedAt).toLocaleString()}
              {s.academic.calculatedAt ? ` · 学校计算时间 ${s.academic.calculatedAt}` : ""}
            </span>
          </div>
        )}
        {calculating && (
          <p className="text-sm text-muted" role="status">
            正在重新计算 {progress}
          </p>
        )}
        {tree ? (
          <>
            <AcademicGraph tree={tree} viewKey={viewKey} onOpen={open} />
            <div className="flex flex-wrap justify-between gap-2 text-xs text-muted">
              <span>拖动画布平移 · 使用左下角按钮缩放 · 点击节点查看课程</span>
              <span>未知数据不计入完成率</span>
            </div>
            {tree.warnings.map((w) => (
              <p key={w} className="text-sm text-warning">
                {w}
              </p>
            ))}
            {selected && tree.nodes.has(selected) && (
              <CourseQueryModal
                key={`${viewKey}:${selected}`}
                tree={tree}
                nodeId={selected}
                viewKey={viewKey}
                onClose={close}
              />
            )}
          </>
        ) : (
          <Card>
            <Card.Content className="grid min-h-[420px] place-content-center justify-items-center gap-4 text-center">
              {pending.academic ? (
                <Spinner aria-label="正在加载学业数据" />
              ) : (
                <>
                  <h2 className="text-lg font-semibold">还没有学业完成数据</h2>
                  <p className="max-w-md text-sm text-muted">
                    完成官方认证后刷新。已有缓存会保留，系统暂时不可用时也能浏览培养方案。
                  </p>
                  <Button onPress={() => void portal("academic")}>打开官方认证</Button>
                </>
              )}
            </Card.Content>
          </Card>
        )}
      </div>
    </PageShell>
  );
}
