import { createContext, useContext, useId, type ReactNode } from "react";

interface NumberFieldContextValue {
  value: number;
  min: number;
  max: number;
  step: number;
  disabled: boolean;
  inputId: string;
  setValue: (next: number) => void;
}

const NumberFieldContext = createContext<NumberFieldContextValue | null>(null);

function useStepper(): NumberFieldContextValue {
  const context = useContext(NumberFieldContext);
  if (!context) throw new Error("NumberField 子组件必须在 NumberField 内使用");
  return context;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

/**
 * 数字步进输入框。
 *
 * material 没有数字输入控件，因此自建。形状沿用 M3 的 outlined text field：
 * 一条描边包住「减号 / 数值 / 加号」，两侧按钮与数值之间用短竖线分隔，
 * 与 material 的图标按钮和输入框保持同一套视觉语言。
 *
 * 默认直接渲染成 `[− | 数值 | +]`；需要自定义结构时（例如把按钮换成别的控件）
 * 可以用 `NumberField.Group` 与 `NumberField.DecrementButton` 等子组件自行拼装。
 */
function NumberStepperRoot({
  children,
  value = 0,
  minValue,
  maxValue,
  min,
  max,
  step = 1,
  onChange,
  disabled,
  isDisabled,
  className,
  label,
  description,
}: {
  children?: ReactNode;
  value?: number;
  minValue?: number;
  maxValue?: number;
  min?: number;
  max?: number;
  step?: number;
  onChange?: (value: number) => void;
  disabled?: boolean;
  isDisabled?: boolean;
  className?: string;
  label?: ReactNode;
  description?: ReactNode;
}) {
  const inputId = useId();
  const lo = minValue ?? min ?? 0;
  const hi = maxValue ?? max ?? Number.MAX_SAFE_INTEGER;
  const isOff = Boolean(disabled || isDisabled);

  return (
    <NumberFieldContext.Provider
      value={{
        value,
        min: lo,
        max: hi,
        step,
        disabled: isOff,
        inputId,
        setValue: (next: number) => {
          const clamped = clamp(next, lo, hi);
          const decimals = String(step).split(".")[1]?.length ?? 0;
          onChange?.(Number(clamped.toFixed(decimals)));
        },
      }}
    >
      <div className={`number-field ${className ?? ""}`}>
        {label && (
          <label htmlFor={inputId} className="field-label">
            {label}
          </label>
        )}
        {children ?? (
          <div className="number-stepper">
            <DecrementButton />
            <Input />
            <IncrementButton />
          </div>
        )}
        {description && <p className="field-description">{description}</p>}
      </div>
    </NumberFieldContext.Provider>
  );
}

const Group = ({ children, className }: { children: ReactNode; className?: string }) => (
  <div className={`number-stepper ${className ?? ""}`}>{children}</div>
);

function DecrementButton() {
  const { value, min, step, disabled, setValue } = useStepper();
  return (
    <button
      type="button"
      className="number-stepper-button"
      aria-label="减少"
      disabled={disabled || value <= min}
      onClick={() => setValue(value - step)}
    >
      −
    </button>
  );
}

function IncrementButton() {
  const { value, max, step, disabled, setValue } = useStepper();
  return (
    <button
      type="button"
      className="number-stepper-button"
      aria-label="增加"
      disabled={disabled || value >= max}
      onClick={() => setValue(value + step)}
    >
      +
    </button>
  );
}

function Input() {
  const { value, min, max, step, disabled, inputId, setValue } = useStepper();
  return (
    <input
      id={inputId}
      type="number"
      className="number-stepper-input"
      value={value}
      min={min}
      max={max}
      step={step}
      disabled={disabled}
      onChange={(event) => {
        const parsed = Number(event.target.value);
        if (Number.isFinite(parsed)) setValue(parsed);
      }}
    />
  );
}

export const NumberField = Object.assign(NumberStepperRoot, {
  Group,
  DecrementButton,
  IncrementButton,
  Input,
});
