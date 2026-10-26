import { Select, SelectOption, Switch } from "material/react";
import { compileOptions, standards, type CompileOptions as Options } from "./compile-options";
export function CompileOptions({
  language,
  value,
  disabled,
  onChange,
}: {
  language: string;
  value: Options | null;
  disabled: boolean;
  onChange: (value: Options) => void;
}) {
  const options = compileOptions(language, value);
  if (!options) return null;
  return (
    <div className="oj-form" role="group" aria-label="编译选项">
      <Select
        label="语言标准"
        value={options.version}
        disabled={disabled}
        onChange={(e: any) => {
          const val = e.detail?.value ?? e.target?.value;
          if (val) onChange({ ...options, version: String(val) });
        }}
      >
        {standards[language].map((standard) => (
          <SelectOption key={standard} value={standard}>
            {standard.toUpperCase()}
          </SelectOption>
        ))}
      </Select>
      <Switch
        checked={options.enableAsan}
        disabled={disabled}
        supportingText="Address Sanitizer"
        onChange={(e: any) => {
          const enableAsan = Boolean(e.detail?.checked ?? e.target?.checked);
          onChange({ ...options, enableAsan, enableLsan: enableAsan && options.enableLsan });
        }}
      >
        ASan · 越界检测
      </Switch>
      <Switch
        checked={options.enableLsan}
        disabled={disabled || !options.enableAsan}
        supportingText="Leak Sanitizer，需先开启 ASan"
        onChange={(e: any) => {
          const enableLsan = Boolean(e.detail?.checked ?? e.target?.checked);
          onChange({ ...options, enableLsan });
        }}
      >
        LSan · 内存泄漏检测
      </Switch>
    </div>
  );
}
