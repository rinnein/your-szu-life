import { useEffect, useMemo, useState } from "react";
import { LuCheck, LuImage, LuPalette, LuRotateCcw } from "react-icons/lu";
import { Button, Card, LoadingIndicator, Radio, Radios, Switch, TextField } from "material/react";
import { Banner, NumberField, toast } from "./ui";
import { useTheme } from "./theme/ThemeProvider";
import {
  MODE_LABELS,
  normalizeSeed,
  SEED_PRESETS,
  VARIANT_LABELS,
  type SchemeVariant,
  type ThemeMode,
} from "./theme/defaults";
import { generateScheme } from "./theme/scheme";
import { getPlatform } from "./os-info";
import { useRegisterNextAction, type StepNextAction } from "./step-next-action";

/**
 * 外观设置。
 *
 * 设置页使用完整形态；初始化向导通过 `onboarding` 复用同一组件，只隐藏
 * 种子色卡片，保留模式与和设置页一致的壁纸动态色彩区块，并把主操作交给
 * 向导底栏。
 *
 * Android 上种子色默认跟随壁纸；一旦在这里手动选色，`seedSource` 变为
 * `user`，壁纸色不再覆盖。点「恢复默认外观」可重新跟随壁纸。
 */
export function AppearanceSettings({
  onboarding = false,
  onComplete,
  registerNext,
}: {
  onboarding?: boolean;
  onComplete?: () => void | Promise<void>;
  registerNext?: (action: StepNextAction | null) => void;
}) {
  const {
    appearance,
    resolvedMode,
    wallpaperAvailable,
    wallpaperListeningSupported,
    setMode,
    setSeed,
    setVariant,
    setAutoWallpaper,
    setWallpaperPollMinutes,
    reset,
    refreshWallpaperSeed,
  } = useTheme();
  const [customSeed, setCustomSeed] = useState(appearance.seed);
  const [busy, setBusy] = useState(false);
  const [platform, setPlatform] = useState("unknown");

  useEffect(() => {
    let active = true;
    void getPlatform()
      .then((value) => {
        if (active) setPlatform(value);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  const seedLocked = Boolean(appearance.autoWallpaper);

  useEffect(() => {
    if (!seedLocked) setCustomSeed(appearance.seed);
  }, [appearance.seed, seedLocked]);

  // 每个配色变体展示一组 4 个颜色（主色/次色/第三色/表面变体），
  // 与 Android Monet 依据变体取色的方式一致。
  const palettePreviews = useMemo(
    () =>
      (Object.keys(VARIANT_LABELS) as SchemeVariant[]).map((variant) => {
        const scheme = generateScheme(
          appearance.seed,
          variant,
          resolvedMode === "dark",
        ) as unknown as Record<string, string>;
        return {
          variant,
          label: VARIANT_LABELS[variant],
          isDefault: variant === "tonal-spot",
          colors: [
            { role: "主色", color: scheme.primary },
            { role: "次色", color: scheme.secondary },
            { role: "第三色", color: scheme.tertiary },
            { role: "表面变体", color: scheme["surface-variant"] },
          ],
        };
      }),
    [appearance.seed, resolvedMode],
  );

  const displayedSeed = seedLocked ? appearance.seed : customSeed;

  const applySeed = (seed: string) => {
    const normalized = normalizeSeed(seed);
    if (!normalized) {
      toast.danger("颜色格式不正确", {
        description: "请输入形如 #5d5fef 的六位十六进制颜色。",
      });
      return;
    }
    setSeed(normalized);
    setCustomSeed(normalized);
  };

  const applyDynamicColor = async () => {
    setBusy(true);
    try {
      await refreshWallpaperSeed();
      toast.success("已跟随当前壁纸动态取色");
    } catch (cause) {
      toast.danger("未能读取当前壁纸", {
        description: typeof cause === "string" ? cause : "可以手动上传一张图片来提取配色。",
      });
    } finally {
      setBusy(false);
    }
  };

  useRegisterNextAction(
    registerNext,
    onboarding && onComplete ? { label: "下一步", disabled: busy, busy, run: onComplete } : null,
  );

  return (
    <div className="appearance-settings">
      <Card variant="outlined">
        <div slot="headline">深浅色模式</div>
        <div slot="subhead">
          当前实际使用{resolvedMode === "dark" ? "深色" : "浅色"}。选择「跟随系统」时随操作系统的
          外观设置自动切换。
        </div>
        <div className="p-4">
          <Radios
            aria-label="深浅色模式"
            value={appearance.mode}
            onChange={(event) => setMode(event.detail.value as ThemeMode)}
          >
            {(Object.keys(MODE_LABELS) as ThemeMode[]).map((mode) => (
              <Radio key={mode} value={mode}>
                {MODE_LABELS[mode]}
              </Radio>
            ))}
          </Radios>
        </div>
      </Card>

      {!onboarding && (
        <Card variant="outlined">
          <div slot="headline">种子色</div>
          <div slot="subhead">
            {seedLocked
              ? "当前跟随壁纸动态色彩，种子色会随壁纸自动更新。"
              : appearance.seedSource === "wallpaper"
                ? "当前取自系统壁纸。手动选择后将以你的选择为准。"
                : appearance.seedSource === "user"
                  ? "当前使用你手动选择的颜色。"
                  : "当前使用应用默认品牌色。"}
          </div>
          <div className="p-4">
            {seedLocked && (
              <Banner status="info">
                <Banner.Title>种子色已锁定</Banner.Title>
                <Banner.Description>
                  关闭「壁纸动态色彩 → 自动从壁纸取色」后可手动选择颜色。
                </Banner.Description>
              </Banner>
            )}
            <div className="seed-grid" role="group" aria-label="预设配色">
              {SEED_PRESETS.map((preset) => (
                <button
                  key={preset.id}
                  type="button"
                  className="seed-swatch"
                  data-selected={appearance.seed === preset.seed || undefined}
                  aria-label={`使用${preset.label}配色`}
                  aria-pressed={appearance.seed === preset.seed}
                  disabled={busy || seedLocked}
                  onClick={() => {
                    setSeed(preset.seed);
                    setCustomSeed(preset.seed);
                  }}
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
                value={displayedSeed}
                onChange={(e: any) => setCustomSeed(e.detail?.value ?? e.target?.value ?? "")}
                supportingText={
                  seedLocked ? "当前壁纸提取的实际颜色" : "六位十六进制，例如 #5d5fef"
                }
                maxlength={7}
                disabled={busy || seedLocked}
              />
              <Button
                variant="tonal"
                disabled={busy || seedLocked}
                onClick={() => applySeed(displayedSeed)}
              >
                <LuPalette aria-hidden="true" />
                应用
              </Button>
            </div>
          </div>
        </Card>
      )}

      {platform !== "unknown" && (
        <Card variant="outlined">
          <div slot="headline">壁纸动态色彩</div>
          <div slot="subhead">
            {platform === "android"
              ? "使用系统壁纸颜色生成 Material You 配色。"
              : "启动时自动读取当前系统壁纸并提取种子色。"}
          </div>
          <div className="p-4 grid gap-3">
            {platform !== "android" && (
              <div className="flex items-center justify-between gap-4">
                <div className="space-y-1">
                  <div className="font-medium text-sm text-foreground">自动从壁纸取色</div>
                  <p className="text-xs text-muted">启动时读取当前系统壁纸并提取种子色。</p>
                </div>
                <Switch
                  checked={appearance.autoWallpaper}
                  onChange={(event: any) =>
                    setAutoWallpaper(Boolean(event.detail?.checked ?? event.target?.checked))
                  }
                  aria-label="自动从壁纸取色"
                />
              </div>
            )}
            {!wallpaperListeningSupported && (
              <Banner status="info">
                <Banner.Title>壁纸变更监视为轮询</Banner.Title>
                <Banner.Description>
                  当前壁纸管理器不支持壁纸更改事件；启用自动取色后，将按
                  {appearance.wallpaperPollMinutes} 分钟的间隔检查壁纸。
                </Banner.Description>
              </Banner>
            )}
            {!wallpaperListeningSupported && (
              <NumberField
                label="检查间隔（分钟）"
                value={appearance.wallpaperPollMinutes}
                minValue={1}
                maxValue={1440}
                step={1}
                disabled={busy}
                onChange={setWallpaperPollMinutes}
                description="范围 1 - 1440 分钟；启用自动取色后按此间隔检查壁纸。"
              />
            )}
            {(platform === "android" || wallpaperAvailable) && (
              <Button variant="outlined" disabled={busy} onClick={() => void applyDynamicColor()}>
                {busy ? <LoadingIndicator size={16} /> : <LuImage aria-hidden="true" />}
                跟随壁纸动态取色
              </Button>
            )}
            {platform !== "android" && appearance.autoWallpaper && !wallpaperAvailable && (
              <>
                <Banner status="warning">
                  <Banner.Title>暂未读取到壁纸颜色</Banner.Title>
                  <Banner.Description>
                    当前壁纸可能不是可识别的图片，或壁纸管理器不可用。现有配色不会被改动；可以更换壁纸后重试。
                  </Banner.Description>
                </Banner>
                <Button variant="outlined" disabled={busy} onClick={() => void applyDynamicColor()}>
                  {busy ? <LoadingIndicator size={16} /> : <LuImage aria-hidden="true" />}
                  重试读取壁纸
                </Button>
              </>
            )}
          </div>
        </Card>
      )}

      {!onboarding && (
        <>
          <Card variant="outlined">
            <div slot="headline">配色变体</div>
            <div slot="subhead">
              Tonal Spot 与 Android 系统一致，是唯一保证 WebView 与原生标题栏同色的选项；
              其它变体会改变色相与饱和度。
            </div>
            <div className="p-4">
              <p className="text-xs text-muted monet-palette-caption">
                每行是一个配色变体，右侧 4 个颜色依次为主色、次色、第三色与表面变体。
              </p>
              <div className="monet-palettes" role="radiogroup" aria-label="配色变体">
                {palettePreviews.map(({ variant, label, isDefault, colors }) => {
                  const selected = appearance.variant === variant;
                  return (
                    <button
                      key={variant}
                      type="button"
                      role="radio"
                      aria-checked={selected}
                      aria-label={label}
                      className="monet-palette"
                      data-selected={selected || undefined}
                      onClick={() => setVariant(variant)}
                    >
                      <span className="monet-palette-label">
                        {label}
                        {isDefault && <span className="monet-palette-default">默认</span>}
                      </span>
                      <span className="monet-palette-colors" aria-hidden="true">
                        {colors.map(({ role, color }) => (
                          <span
                            key={role}
                            className="monet-swatch"
                            style={{ background: color }}
                            title={`${role} ${color}`}
                          />
                        ))}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          </Card>

          {appearance.seedSource === "user" && (
            <Banner status="info">
              <Banner.Title>壁纸配色已暂停</Banner.Title>
              <Banner.Description>
                你手动选择了配色，Android 壁纸变化不会再覆盖它。恢复默认即可重新跟随壁纸。
              </Banner.Description>
            </Banner>
          )}

          <div className="appearance-reset">
            <Button
              variant="outlined"
              onClick={() => {
                reset();
                setCustomSeed("");
                toast.success("已恢复默认外观");
              }}
            >
              <LuRotateCcw aria-hidden="true" />
              恢复默认外观
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
