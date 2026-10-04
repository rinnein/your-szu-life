import { Description, Label, ListBox, Select, Switch } from "@heroui/react";
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
        value={options.version}
        onChange={(key) => onChange({ ...options, version: String(key) })}
        isDisabled={disabled}
      >
        <Label>语言标准</Label>
        <Select.Trigger>
          <Select.Value />
          <Select.Indicator />
        </Select.Trigger>
        <Select.Popover>
          <ListBox>
            {standards[language].map((standard) => (
              <ListBox.Item key={standard} id={standard} textValue={standard.toUpperCase()}>
                {standard.toUpperCase()}
                <ListBox.ItemIndicator />
              </ListBox.Item>
            ))}
          </ListBox>
        </Select.Popover>
      </Select>
      <Switch
        isSelected={options.enableAsan}
        isDisabled={disabled}
        onChange={(enableAsan) =>
          onChange({ ...options, enableAsan, enableLsan: enableAsan && options.enableLsan })
        }
      >
        <Switch.Content>
          <Switch.Control>
            <Switch.Thumb />
          </Switch.Control>
          ASan · 越界检测
        </Switch.Content>
        <Description>Address Sanitizer</Description>
      </Switch>
      <Switch
        isSelected={options.enableLsan}
        isDisabled={disabled || !options.enableAsan}
        onChange={(enableLsan) => onChange({ ...options, enableLsan })}
      >
        <Switch.Content>
          <Switch.Control>
            <Switch.Thumb />
          </Switch.Control>
          LSan · 内存泄漏检测
        </Switch.Content>
        <Description>Leak Sanitizer，需先开启 ASan</Description>
      </Switch>
    </div>
  );
}
