import { useEffect, useRef, useState } from "react";
import { Card, LoadingIndicator } from "material/react";
import { Chip } from "../ui";
import { LuGlobe, LuSchool } from "react-icons/lu";
import { useEnrollment } from "./use-enrollment";
import { defaultMode, type Probe } from "./model";
import { OperationError } from "./shared";
export default function AccessModes({ autoSelect = false }: { autoSelect?: boolean }) {
  const { snapshot: s, run, portal, pending, errors } = useEnrollment();
  const [probes, setProbes] = useState<Probe[]>([]);
  const manual = useRef(false);
  const choosing = useRef(false);
  const current = useRef(s);
  current.current = s;
  useEffect(() => {
    let live = true;
    if (!s) return;
    void run<Probe[]>({ type: "probe" }).then((p) => {
      if (!live || !p) return;
      setProbes(p);
      if (
        autoSelect &&
        !manual.current &&
        !current.current?.loggedIn &&
        !current.current?.running &&
        !current.current?.tasks.some((t) => t.pending)
      ) {
        const mode = defaultMode(p);
        if (mode !== current.current?.mode) void run({ type: "mode", mode });
      }
    });
    return () => {
      live = false;
    };
  }, [!!s, s?.message === "认证成功", autoSelect, run]);
  const choose = async (mode: "campus" | "webvpn") => {
    if (choosing.current) return;
    choosing.current = true;
    manual.current = true;
    try {
      if (mode !== current.current?.mode) {
        const changed = await run({ type: "mode", mode });
        if (!changed) return;
      }
      const prior = probes.find((p) => p.mode === mode);
      if (mode === "webvpn" && prior?.needs2fa) {
        await portal("webvpn");
        return;
      }
      const p = await run<Probe[]>({ type: "probe" });
      if (p) {
        setProbes(p);
        if (mode === "webvpn" && p.find((p) => p.mode === mode)?.needs2fa) await portal("webvpn");
      }
    } finally {
      choosing.current = false;
    }
  };
  return (
    <div className="grid gap-3">
      <div role="radiogroup" aria-label="选课访问源" className="grid gap-3 sm:grid-cols-2">
        {(["campus", "webvpn"] as const).map((mode) => {
          const p = probes.find((p) => p.mode === mode);
          const Icon = mode === "campus" ? LuSchool : LuGlobe;
          const isSelected = s?.mode === mode;
          const isDisabled = !!pending.mode || !!pending["portal-webvpn"];
          return (
            <label
              key={mode}
              role="radio"
              aria-checked={isSelected}
              aria-label={mode === "campus" ? "校内访问" : "WebVPN访问"}
              tabIndex={isDisabled ? -1 : 0}
              className="block w-full cursor-pointer rounded-2xl outline-none has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent data-[disabled]:cursor-wait data-[disabled]:opacity-60"
              onClick={() => {
                if (!isDisabled) void choose(mode);
              }}
              onKeyDown={(e) => {
                if ((e.key === "Enter" || e.key === " ") && !isDisabled) {
                  e.preventDefault();
                  void choose(mode);
                }
              }}
            >
              <Card
                variant="outlined"
                className={`h-full ${isSelected ? "ring-2 ring-accent" : ""}`}
              >
                <div className="grid gap-3 p-4">
                  <div className="flex items-center justify-between">
                    <Icon className="size-6 text-accent" />
                    {isSelected && (
                      <Chip size="sm" variant="soft">
                        当前选择
                      </Chip>
                    )}
                  </div>
                  <span className="text-base font-semibold">
                    {mode === "campus" ? "校内访问" : "WebVPN访问"}
                  </span>
                  <p className="text-sm text-muted">
                    {p?.needs2fa
                      ? "需2FA验证"
                      : p?.latency != null
                        ? `${p.latency} ms`
                        : pending.probe
                          ? "正在测试连接…"
                          : p?.message || "尚未测试"}
                  </p>
                </div>
              </Card>
            </label>
          );
        })}
      </div>
      <OperationError message={errors.mode || errors.probe || errors["portal-webvpn"]} />
      {pending["portal-webvpn"] && <LoadingIndicator size={24} aria-label="正在打开2FA认证" />}
    </div>
  );
}
