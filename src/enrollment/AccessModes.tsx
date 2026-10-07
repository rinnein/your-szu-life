import { useEffect, useRef, useState } from "react";
import { Card, Chip, Spinner } from "@heroui/react";
import { Radio, RadioGroup } from "react-aria-components";
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
      <RadioGroup
        aria-label="选课访问源"
        value={s?.mode ?? "campus"}
        onChange={(mode) => void choose(mode as "campus" | "webvpn")}
        isDisabled={!!pending.mode || !!pending["portal-webvpn"]}
        className="grid gap-3 sm:grid-cols-2"
      >
        {(["campus", "webvpn"] as const).map((mode) => {
          const p = probes.find((p) => p.mode === mode);
          const Icon = mode === "campus" ? LuSchool : LuGlobe;
          return (
            <Radio
              key={mode}
              value={mode}
              aria-label={mode === "campus" ? "校内访问" : "WebVPN访问"}
              className="block w-full cursor-pointer rounded-2xl outline-none data-[focus-visible]:ring-2 data-[focus-visible]:ring-accent data-[disabled]:cursor-wait data-[disabled]:opacity-60"
              onClick={() => {
                if (mode === s?.mode && !pending.mode && !pending["portal-webvpn"])
                  void choose(mode);
              }}
            >
              <Card className={`h-full ${s?.mode === mode ? "ring-2 ring-accent" : ""}`}>
                <Card.Content className="gap-3">
                  <div className="flex items-center justify-between">
                    <Icon className="size-6 text-accent" />
                    {s?.mode === mode && (
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
                </Card.Content>
              </Card>
            </Radio>
          );
        })}
      </RadioGroup>
      <OperationError message={errors.mode || errors.probe || errors["portal-webvpn"]} />
      {pending["portal-webvpn"] && <Spinner aria-label="正在打开2FA认证" />}
    </div>
  );
}
