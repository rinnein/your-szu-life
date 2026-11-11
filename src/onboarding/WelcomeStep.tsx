import { Card } from "material/react";
import { BrandMark } from "../components";
import { useRegisterNextAction } from "../step-next-action";
import { useDemoModeActivation } from "../shell/use-demo-mode";
import type { StepProps } from "./step-types";

/**
 * 初始化向导的欢迎页：只在新装（或重启向导）时出现。像首页的调试模式一样，
 * 2 秒内连击 Logo 5 次进入演示模式（同时开启调试模式），直达主应用检查样式。
 */
export function WelcomeStep({ onComplete, registerNext }: StepProps) {
  const handleBrandTap = useDemoModeActivation();

  useRegisterNextAction(registerNext, {
    label: "开始配置",
    disabled: false,
    busy: false,
    run: onComplete,
  });

  return (
    <Card variant="outlined">
      <div slot="headline">欢迎使用「泥深」</div>
      <div slot="subhead">接下来只需几步，完成账户、文件位置与外观等初始设置。</div>
      <div className="p-6 flex flex-col items-center text-center gap-3">
        <button
          type="button"
          className="brand-mark-trigger"
          aria-label="应用图标，连续点击 5 次进入演示模式"
          onClick={handleBrandTap}
        >
          <BrandMark large />
        </button>
        <p className="text-sm text-muted">点击「开始配置」进入下一步。</p>
      </div>
    </Card>
  );
}
