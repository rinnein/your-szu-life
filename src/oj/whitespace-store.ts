import { Store, useStore } from "@tanstack/react-store";

/**
 * 全局共享的空格与制表符可见性状态。
 * 所有题目样例（SampleField）和测试点输入/输出共用此状态。
 */
export const whitespaceStore = new Store<boolean>(false);

export const useWhitespaceVisible = () => useStore(whitespaceStore);

export const setWhitespaceVisible = (visible: boolean | ((prev: boolean) => boolean)) => {
  whitespaceStore.setState((prev) => (typeof visible === "function" ? visible(prev) : visible));
};

export const toggleWhitespaceVisible = () => {
  whitespaceStore.setState((prev) => !prev);
};

export const resetWhitespaceVisible = () => {
  whitespaceStore.setState(() => false);
};
