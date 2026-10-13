import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { Button, Spinner, toast } from "@heroui/react";
import { useEnrollment } from "./use-enrollment";
import type { Snapshot } from "./model";
type Point = [number, number];
type Reply = {
  status: string;
  challengeId?: string;
  image?: string;
  revision?: number;
  index?: number;
  point?: Point;
  snapshot?: Snapshot;
  message?: string;
};
type View = {
  image?: string;
  id?: string;
  points: Point[];
  phase: "loading" | "auto" | "manual" | "submitting";
  retries: number;
  message: string;
};
const paint = () =>
  new Promise<void>((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
  );
export default function CaptchaInput({
  onAuthenticated,
  onModelRequired,
}: {
  onAuthenticated: (s: Snapshot) => void;
  onModelRequired?: () => void;
}) {
  const { run, errors } = useEnrollment();
  const [view, setView] = useState<View>({
    points: [],
    phase: "loading",
    retries: 0,
    message: "正在获取验证码",
  });
  const region = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (view.image) region.current?.scrollIntoView?.({ block: "nearest" });
  }, [view.image]);
  const current = useRef(view);
  const revision = useRef(0);
  const flow = useRef(crypto.randomUUID());
  const alive = useRef(true);
  const notify = useRef(onAuthenticated);
  notify.current = onAuthenticated;
  const update = (patch: Partial<View>) => {
    current.current = { ...current.current, ...patch };
    if (alive.current) setView(current.current);
  };
  const valid = (r: number) => alive.current && revision.current === r;
  const request = (type: string, r: number, extra: Record<string, unknown> = {}) =>
    run<Reply>({ type, flow: flow.current, revision: r, ...extra }, "captchaFlow");
  const cancel = (discard = false) => {
    const r = ++revision.current;
    void invoke("enrollment_command", {
      action: { type: "captchaCancel", flow: flow.current, revision: r, discard },
    }).catch(() => {});
    return r;
  };
  const lastFlowErrorRef = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (errors.captchaFlow && errors.captchaFlow !== lastFlowErrorRef.current) {
      lastFlowErrorRef.current = errors.captchaFlow;
      toast.danger(errors.captchaFlow);
    }
  }, [errors.captchaFlow]);

  const authenticated = (reply: Reply | undefined) => {
    if (reply?.status === "authenticated" && reply.snapshot?.loggedIn) {
      notify.current(reply.snapshot);
      return true;
    }
    return false;
  };
  const prepare = async (r: number, automatic: boolean) => {
    update({
      phase: "loading",
      id: undefined,
      image: undefined,
      points: [],
      message: "正在获取验证码",
    });
    const reply = await request("captchaPrepare", r);
    if (!valid(r)) return false;
    if (reply?.status === "modelMissing") {
      onModelRequired?.();
      update({ phase: "manual", message: "请先安装本地 OCR 模型" });
      return false;
    }
    if (authenticated(reply)) return false;
    if (reply?.status !== "challenge" || !reply.image || !reply.challengeId) {
      update({ phase: "manual", message: "获取未完成，可重新获取验证码", id: undefined });
      return false;
    }
    update({
      id: reply.challengeId,
      image: reply.image,
      points: [],
      phase: automatic ? "auto" : "manual",
      message: automatic ? "正在自动识别" : "按顶部顺序点选四个字",
    });
    await paint();
    return valid(r);
  };
  const submit = async (r: number, automatic: boolean) => {
    if (
      !valid(r) ||
      !current.current.id ||
      current.current.points.length !== 4 ||
      current.current.phase === "submitting"
    )
      return "stop";
    update({ phase: "submitting", message: "正在提交登录" });
    const reply = await request("captchaSubmit", r, {
      challengeId: current.current.id,
      points: current.current.points,
    });
    if (!valid(r)) return "stop";
    if (authenticated(reply)) return "stop";
    update({
      id: undefined,
      phase: automatic ? "auto" : "manual",
      message: reply?.message ?? "本次登录未完成，请重新检查登录状态",
    });
    if (reply?.status === "rejected") {
      if (automatic) return "retry";
      await prepare(r, false);
    }
    return "stop";
  };
  const automate = async (r: number, needsImage: boolean) => {
    let structural = 0;
    if (needsImage && !(await prepare(r, true))) return;
    while (valid(r) && current.current.id) {
      update({ phase: "auto", message: "正在自动识别，点击图片即可接管" });
      const id = current.current.id;
      const accepted = [...current.current.points];
      const reply = await request("captchaPoint", r, { challengeId: id, points: accepted });
      if (!valid(r)) return;
      if (reply?.status === "modelMissing") {
        onModelRequired?.();
        update({ phase: "manual", message: "请先安装本地 OCR 模型" });
        return;
      }
      let retry = false;
      if (
        reply?.status === "point" &&
        reply.challengeId === id &&
        reply.revision === r &&
        reply.index === accepted.length &&
        reply.point
      ) {
        if (current.current.points.length !== accepted.length) return;
        structural = 0;
        update({ points: [...accepted, reply.point] });
        if (current.current.points.length === 4) retry = (await submit(r, true)) === "retry";
        else {
          await paint();
          continue;
        }
        if (!retry) return;
      } else if (reply?.status === "structural" || reply?.status === "uncertain") {
        structural = reply.status === "structural" ? structural + 1 : 0;
        retry = true;
      } else {
        update({ phase: "manual", message: "自动识别已停止，可手动点选" });
        return;
      }
      if (retry) {
        if (structural >= 3 || current.current.retries >= 4) {
          if (!current.current.id && !(await prepare(r, false))) return;
          update({ phase: "manual", message: "自动识别未完成，请手动点选" });
          return;
        }
        update({ retries: current.current.retries + 1 });
        if (!(await prepare(r, true))) return;
      }
    }
  };
  useEffect(() => {
    alive.current = true;
    const r = ++revision.current;
    void automate(r, true);
    return () => {
      alive.current = false;
      cancel(true);
    };
  }, []);
  const locked = view.phase === "submitting" || view.phase === "loading";
  return (
    <div
      ref={region}
      className="grid justify-items-center gap-3 rounded-xl bg-default p-4"
      aria-label="验证码填写"
    >
      <p role="status" className="text-sm">
        {view.message}
      </p>
      {view.image ? (
        <button
          type="button"
          disabled={locked || !view.id}
          aria-label="点选验证码"
          className="relative h-[80px] w-[250px] disabled:cursor-wait"
          onClick={(e) => {
            if (
              current.current.phase === "submitting" ||
              !current.current.id ||
              current.current.points.length >= 4
            )
              return;
            const r = cancel();
            const box = e.currentTarget.getBoundingClientRect();
            const point: Point = [
              Math.min(249, Math.max(0, Math.floor(((e.clientX - box.left) * 250) / box.width))),
              Math.min(79, Math.max(0, Math.floor(((e.clientY - box.top) * 80) / box.height))),
            ];
            update({ phase: "manual", message: "手动点选：满四个点后自动登录" });
            if (current.current.points.some((p) => p[0] === point[0] && p[1] === point[1])) return;
            update({ points: [...current.current.points, point] });
            if (current.current.points.length === 4) void submit(r, false);
          }}
        >
          <img src={view.image} width={250} height={80} alt="学校点选验证码" draggable={false} />
          {view.points.map(([x, y], i) => (
            <span
              key={i}
              className="pointer-events-none absolute rounded bg-accent px-1 text-accent-foreground"
              style={{ left: x, top: y }}
              aria-label={`标记 ${i + 1}`}
            >
              {i + 1}
            </span>
          ))}
        </button>
      ) : view.phase === "loading" ? (
        <Spinner aria-label="正在获取验证码" />
      ) : null}
      <div className="flex flex-wrap items-center justify-center gap-2">
        <Button
          size="sm"
          variant="secondary"
          isDisabled={locked || view.phase === "auto"}
          onPress={() => {
            const r = cancel();
            update({ retries: 0, phase: "auto" });
            void automate(r, !current.current.id);
          }}
        >
          自动识别
        </Button>
        <span aria-label="验证码重试次数" className="text-xs text-muted">
          已重试 {view.retries} 次
        </span>
        <Button
          size="sm"
          variant="tertiary"
          isDisabled={locked}
          onPress={() => {
            cancel();
            update({ points: [], phase: "manual", message: "按顶部顺序点选四个字" });
          }}
        >
          重选
        </Button>
        <Button
          size="sm"
          variant="tertiary"
          isDisabled={locked}
          onPress={() => {
            const r = cancel();
            void prepare(r, false);
          }}
        >
          换一张
        </Button>
      </div>
    </div>
  );
}
