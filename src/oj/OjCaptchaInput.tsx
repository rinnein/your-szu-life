import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { Card, LoadingIndicator, TextField, Button } from "material/react";
import { toast } from "../ui";
import { LuLogIn, LuRefreshCw, LuSparkles } from "react-icons/lu";
import { useOcrModelStatus } from "../OcrModelSettings";
import type { Site } from "./types";

export interface OjCaptchaInputProps {
  site: Site;
  onAuthenticated?: () => void;
  onModelRequired?: () => void;
  className?: string;
}

export default function OjCaptchaInput({
  site,
  onAuthenticated,
  onModelRequired,
  className,
}: OjCaptchaInputProps) {
  const ocrStatus = useOcrModelStatus();
  const [captcha, setCaptcha] = useState<{ token: string; image: string }>();
  const [answer, setAnswer] = useState("");
  const [phase, setPhase] = useState<"loading" | "ready" | "submitting">("loading");
  const [recognizing, setRecognizing] = useState(false);
  const [message, setMessage] = useState("正在获取验证码");
  const alive = useRef(true);

  const fetchCaptcha = async (refresh: boolean) => {
    setPhase("loading");
    setMessage(refresh ? "正在刷新验证码..." : "正在获取验证码...");
    setAnswer("");
    try {
      const res = await invoke<{ token: string; image: string }>("get_oj_captcha", {
        siteId: site.id,
        refresh,
      });
      if (alive.current) {
        setCaptcha(res);
        setPhase("ready");
        setMessage("请输入图片中的验证码");
      }
    } catch (e) {
      if (alive.current) {
        setPhase("ready");
        const msg = String(e);
        setMessage("获取验证码未完成，请重试");
        toast.danger("获取验证码失败", { description: msg });
      }
    }
  };

  useEffect(() => {
    alive.current = true;
    void fetchCaptcha(false);
    return () => {
      alive.current = false;
    };
  }, [site.id]);

  const handleAutoRecognize = async () => {
    if (ocrStatus?.status !== "ready") {
      onModelRequired?.();
      toast.warning("请先安装本地 OCR 模型后使用自动识别");
      return;
    }
    setRecognizing(true);
    setMessage("正在自动识别验证码...");
    try {
      const code = await invoke<string>("auto_recognize_oj_captcha", { siteId: site.id });
      if (alive.current) {
        setAnswer(code);
        setMessage("已自动识别验证码，可直接点击下方登录");
        toast.success(`自动识别完成：${code}`);
      }
    } catch (e) {
      if (alive.current) {
        const error = String(e);
        if (error.includes("ocr_model_missing") || error.includes("ocr_model_invalid")) {
          onModelRequired?.();
          toast.warning("本地 OCR 模型缺失或损坏，请重新安装");
        } else {
          toast.warning("自动识别未完成，请手动输入或点击换一张");
          setMessage("自动识别未完成，请手动输入验证码");
        }
      }
    } finally {
      if (alive.current) setRecognizing(false);
    }
  };

  const handleLogin = async () => {
    if (!answer.trim() || !captcha || phase === "submitting") return;
    setPhase("submitting");
    setMessage("正在提交登录...");
    try {
      await invoke("login_oj", {
        siteId: site.id,
        token: captcha.token,
        value: answer.trim(),
      });
      toast.success("OJ 登录成功");
      onAuthenticated?.();
    } catch (e) {
      const msg = String(e);
      toast.danger("登录失败", { description: msg });
      if (alive.current) {
        setMessage("登录未完成，请重新输入验证码");
        void fetchCaptcha(true);
      }
    }
  };

  const locked = phase === "loading" || phase === "submitting" || recognizing;

  return (
    <Card className={className ?? "max-w-md w-full"} variant="outlined" aria-label="OJ 验证码登录">
      <div slot="headline" className="text-base flex items-center justify-between w-full">
        <span className="font-semibold">验证码登录</span>
        <span className="text-xs font-normal text-muted">
          {site.username} · {site.name}
        </span>
      </div>
      <div slot="subhead" role="status">
        {message}
      </div>
      <div className="grid gap-4 p-4">
        {/* 验证码图片显示框旁边放 换一张、自动识别 */}
        <div className="flex flex-wrap items-center gap-3">
          <div className="relative flex items-center justify-center min-h-[50px] min-w-[150px] h-[60px] w-[180px] rounded-lg border border-default-200 bg-default-100 overflow-hidden">
            {phase === "loading" ? (
              <LoadingIndicator size={16} aria-label="正在获取验证码" />
            ) : captcha?.image ? (
              <img
                src={captcha.image}
                alt="OJ 登录验证码"
                width={180}
                height={60}
                className="object-contain"
                draggable={false}
              />
            ) : (
              <span className="text-xs text-muted">暂无验证码</span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <Button
              size="xs"
              variant="tonal"
              disabled={locked}
              onClick={() => void fetchCaptcha(true)}
            >
              <LuRefreshCw aria-hidden="true" />
              换一张
            </Button>
            <Button
              size="xs"
              variant="tonal"
              disabled={locked || !captcha?.image || recognizing}
              onClick={() => void handleAutoRecognize()}
            >
              <LuSparkles aria-hidden="true" />
              自动识别
            </Button>
          </div>
        </div>

        {/* 验证码输入框 */}
        <TextField
          label="验证码"
          value={answer}
          onChange={(e: any) => setAnswer(e.detail?.value ?? e.target?.value ?? "")}
          disabled={locked}
          autocomplete="off"
          placeholder="输入图片中的验证码"
          onKeyDown={(event) => {
            if (event.key === "Enter" && answer.trim() && !locked) void handleLogin();
          }}
        />

        {/* 输入框下面放登录 */}
        <Button
          variant="filled"
          disabled={locked || !answer.trim()}
          onClick={() => void handleLogin()}
        >
          <LuLogIn aria-hidden="true" />
          登录
        </Button>
      </div>
    </Card>
  );
}
