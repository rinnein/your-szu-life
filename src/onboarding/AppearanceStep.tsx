import { useEffect, useRef, useState } from "react";
import { sourceColorFromImage } from "@material/material-color-utilities";
import { Button, Card, LoadingIndicator, Radio, Radios, TextField } from "material/react";
import { LuCheck, LuImage, LuPalette, LuUpload } from "react-icons/lu";
import { toast } from "../ui";
import { useTheme } from "../theme/ThemeProvider";
import { MODE_LABELS, normalizeSeed, SEED_PRESETS, type ThemeMode } from "../theme/defaults";
import { getPlatform } from "../os-info";
import { getWallpaperImage } from "./api";
import type { StepProps } from "./step-types";

const MAX_UPLOAD_BYTES = 20 * 1024 * 1024;

async function seedFromDataUrl(dataUrl: string): Promise<string> {
  const image = new Image();
  image.src = dataUrl;
  await image.decode();
  const argb = await sourceColorFromImage(image);
  return `#${(argb & 0xffffff).toString(16).padStart(6, "0")}`;
}

export function AppearanceStep({ onComplete }: StepProps) {
  const { appearance, setMode, setSeed, followWallpaper } = useTheme();
  const [customSeed, setCustomSeed] = useState(appearance.seed);
  const [platform, setPlatform] = useState("unknown");
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    void getPlatform()
      .then(setPlatform)
      .catch(() => undefined);
  }, []);

  const applySeed = (seed: string) => {
    const normalized = normalizeSeed(seed);
    if (!normalized) {
      toast.danger("颜色格式不正确", { description: "请输入形如 #5d5fef 的六位十六进制颜色。" });
      return;
    }
    setSeed(normalized);
    setCustomSeed(normalized);
  };

  const grabWallpaperSeed = async () => {
    setBusy(true);
    try {
      const image = await getWallpaperImage();
      applySeed(await seedFromDataUrl(image.dataUrl));
      toast.success("已从当前壁纸提取配色");
    } catch (cause) {
      toast.danger("未能读取当前壁纸", {
        description: typeof cause === "string" ? cause : "可以手动上传一张图片来提取配色。",
      });
    } finally {
      setBusy(false);
    }
  };

  const uploadWallpaper = async (file: File) => {
    if (!file.type.startsWith("image/")) {
      toast.danger("请选择图片文件");
      return;
    }
    if (file.size > MAX_UPLOAD_BYTES) {
      toast.danger("图片过大", { description: "请选择小于 20MB 的图片。" });
      return;
    }
    setBusy(true);
    const url = URL.createObjectURL(file);
    try {
      applySeed(await seedFromDataUrl(url));
      toast.success("已从上传图片提取配色");
    } catch {
      toast.danger("未能从这张图片提取配色", { description: "请换一张图片。" });
    } finally {
      URL.revokeObjectURL(url);
      setBusy(false);
    }
  };

  const followSystemWallpaper = async () => {
    setBusy(true);
    try {
      await followWallpaper();
      toast.success("已切换为跟随壁纸动态取色");
    } catch (cause) {
      toast.danger("当前系统暂时无法读取壁纸颜色", {
        description: typeof cause === "string" ? cause : "请改用手动选择配色。",
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card variant="outlined">
      <div slot="headline">主题设置</div>
      <div slot="subhead">
        选择深浅色模式与配色。Android 可跟随壁纸动态取色；其它平台可从当前壁纸或上传图片提取种子色。
      </div>
      <div className="p-4 grid gap-5">
        <section className="grid gap-2">
          <h3 className="text-sm font-medium text-foreground">深浅色模式</h3>
          <Radios
            aria-label="深浅色模式"
            value={appearance.mode}
            onChange={(event: any) => setMode(event.detail.value as ThemeMode)}
          >
            {(Object.keys(MODE_LABELS) as ThemeMode[]).map((mode) => (
              <Radio key={mode} value={mode}>
                {MODE_LABELS[mode]}
              </Radio>
            ))}
          </Radios>
        </section>

        <section className="grid gap-2">
          <h3 className="text-sm font-medium text-foreground">配色</h3>
          <div className="seed-grid" role="group" aria-label="预设配色">
            {SEED_PRESETS.map((preset) => (
              <button
                key={preset.id}
                type="button"
                className="seed-swatch"
                data-selected={appearance.seed === preset.seed || undefined}
                aria-label={`使用${preset.label}配色`}
                aria-pressed={appearance.seed === preset.seed}
                onClick={() => applySeed(preset.seed)}
              >
                <span className="seed-swatch-color" style={{ background: preset.seed }}>
                  {appearance.seed === preset.seed && (
                    <LuCheck aria-hidden="true" size={16} className="seed-swatch-check" />
                  )}
                </span>
                <span className="seed-swatch-label">{preset.label}</span>
              </button>
            ))}
          </div>
          <div className="seed-custom">
            <TextField
              label="自定义颜色"
              value={customSeed}
              supportingText="六位十六进制，例如 #5d5fef"
              maxlength={7}
              disabled={busy}
              onChange={(e: any) => setCustomSeed(e.detail?.value ?? e.target?.value ?? "")}
            />
            <Button variant="tonal" disabled={busy} onClick={() => applySeed(customSeed)}>
              <LuPalette aria-hidden="true" />
              应用
            </Button>
          </div>
        </section>

        {platform === "android" ? (
          <section className="grid gap-2">
            <h3 className="text-sm font-medium text-foreground">动态色彩</h3>
            <Button variant="outlined" disabled={busy} onClick={() => void followSystemWallpaper()}>
              <LuImage aria-hidden="true" />
              跟随壁纸动态取色
            </Button>
            <p className="text-xs text-muted">
              使用系统壁纸颜色生成 Material You 配色；手动选择配色后会停止跟随。
            </p>
          </section>
        ) : (
          <section className="grid gap-2">
            <h3 className="text-sm font-medium text-foreground">从壁纸取色</h3>
            <div className="flex flex-wrap items-center gap-2">
              <Button variant="outlined" disabled={busy} onClick={() => void grabWallpaperSeed()}>
                {busy ? <LoadingIndicator size={16} /> : <LuImage aria-hidden="true" />}
                从当前壁纸取色
              </Button>
              <Button variant="outlined" disabled={busy} onClick={() => fileRef.current?.click()}>
                <LuUpload aria-hidden="true" />
                上传壁纸图片
              </Button>
              <input
                ref={fileRef}
                type="file"
                accept="image/*"
                hidden
                onChange={(event) => {
                  const file = event.target.files?.[0];
                  event.target.value = "";
                  if (file) void uploadWallpaper(file);
                }}
              />
            </div>
            <p className="text-xs text-muted">截图、照片或壁纸都可以；会自动提取最合适的种子色。</p>
          </section>
        )}

        <Button variant="filled" disabled={busy} onClick={() => void onComplete()}>
          使用此配色
        </Button>
      </div>
    </Card>
  );
}
