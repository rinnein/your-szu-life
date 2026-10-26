import { useState } from "react";
import { LuCheck, LuPalette, LuRotateCcw } from "react-icons/lu";
import { Card, Radio, Radios, TextField, Button } from "material/react";
import { Banner, toast } from "./ui";
import { useTheme } from "./theme/ThemeProvider";
import {
  MODE_LABELS,
  normalizeSeed,
  SEED_PRESETS,
  VARIANT_LABELS,
  type SchemeVariant,
  type ThemeMode,
} from "./theme/defaults";

/**
 * 外观设置。
 *
 * Android 上种子色默认跟随壁纸；一旦在这里手动选色，`seedSource` 变为
 * `user`，壁纸色不再覆盖。点「恢复默认外观」可重新跟随壁纸。
 */
export function AppearanceSettings() {
  const { appearance, resolvedMode, setMode, setSeed, setVariant, reset } = useTheme();
  const [customSeed, setCustomSeed] = useState(appearance.seed);

  const applyCustomSeed = () => {
    const normalized = normalizeSeed(customSeed);
    if (!normalized) {
      toast.danger("颜色格式不正确", {
        description: "请输入形如 #5d5fef 的六位十六进制颜色。",
      });
      return;
    }
    setSeed(normalized);
    toast.success("已应用自定义配色");
  };

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

      <Card variant="outlined">
        <div slot="headline">种子色</div>
        <div slot="subhead">
          {appearance.seedSource === "wallpaper"
            ? "当前取自 Android 壁纸。手动选择后将以你的选择为准。"
            : appearance.seedSource === "user"
              ? "当前使用你手动选择的颜色。"
              : "当前使用应用默认品牌色。"}
        </div>
        <div className="p-4">
          <div className="seed-grid" role="group" aria-label="预设配色">
            {SEED_PRESETS.map((preset) => (
              <button
                key={preset.id}
                type="button"
                className="seed-swatch"
                data-selected={appearance.seed === preset.seed || undefined}
                aria-label={`使用${preset.label}配色`}
                aria-pressed={appearance.seed === preset.seed}
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
              value={customSeed}
              onChange={(e: any) => setCustomSeed(e.detail?.value ?? e.target?.value ?? "")}
              supportingText="六位十六进制，例如 #5d5fef"
              maxlength={7}
            />
            <Button variant="tonal" onClick={applyCustomSeed}>
              <LuPalette aria-hidden="true" />
              应用
            </Button>
          </div>
        </div>
      </Card>

      <Card variant="outlined">
        <div slot="headline">配色变体</div>
        <div slot="subhead">
          Tonal Spot 与 Android 系统一致，是唯一保证 WebView 与原生标题栏同色的选项；
          其它变体会改变色相与饱和度。
        </div>
        <div className="p-4">
          <Radios
            aria-label="配色变体"
            value={appearance.variant}
            onChange={(event) => setVariant(event.detail.value as SchemeVariant)}
          >
            {(Object.keys(VARIANT_LABELS) as SchemeVariant[]).map((variant) => (
              <Radio key={variant} value={variant}>
                {VARIANT_LABELS[variant]}
              </Radio>
            ))}
          </Radios>
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
    </div>
  );
}
