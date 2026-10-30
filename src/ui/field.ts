/**
 * 受控 material `TextField` 的按键绑定。
 *
 * material 的 React 适配器把 `onChange` 映射到元素自己的 `change`（原生 change，
 * 只在失焦/回车时触发），却在每个原生 `input` 事件后用上一次渲染的受控 `value`
 * 回写内部控件。只挂 `onChange` 的受控输入框会把每次键入立刻抹掉；`onInput` 必须
 * 实时更新状态，`onChange` 继续兜住失焦/回车以及测试里直接派发的 change。
 *
 * 用法：
 *   const [name, setName] = useState("");
 *   <TextField value={name} {...bindField(setName)} />
 *
 * 需要顺带做别的事（重置分页、标记已编辑）时把副作用放进 updater：
 *   <TextField value={query} {...bindField((value) => { setQuery(value); setPage(1); })} />
 *
 * 键入与提交要分开处理（例如键入只改草稿、失焦才校验并落库）时传两个 updater：
 *   <TextField value={room} {...bindField({ input: setDraft, change: commit })} />
 */
type FieldValueHandler = (value: string) => void;

export type FieldBindings = {
  onInput?: (event: any) => void;
  onChange?: (event: any) => void;
};

export function bindField(
  update: FieldValueHandler | { input?: FieldValueHandler; change?: FieldValueHandler },
): FieldBindings {
  const read = (event: any): string | undefined => {
    const value = event.detail?.value ?? event.target?.value;
    return typeof value === "string" ? value : undefined;
  };
  const input = typeof update === "function" ? update : update.input;
  const change = typeof update === "function" ? update : update.change;
  const bindings: FieldBindings = {};
  if (input) {
    bindings.onInput = (event) => {
      const value = read(event);
      if (value !== undefined) input(value);
    };
  }
  if (change) {
    bindings.onChange = (event) => {
      const value = read(event);
      if (value !== undefined) change(value);
    };
  }
  return bindings;
}
