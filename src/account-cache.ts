/**
 * 进程内、按账号隔离的单值缓存。
 *
 * 只保留最近一个账号的一份数据：账号不匹配、或写入/读取时还没有账号，
 * 一律视为未命中。这样退出登录或切换账号后，不会把上一个账号的数据
 * 交给新账号，也不需要额外的全局清理入口。
 */
export interface AccountCache<T> {
  read(account: string | undefined): T | undefined;
  write(account: string | undefined, value: T): void;
}

export function createAccountCache<T>(): AccountCache<T> {
  let entry: { account: string; value: T } | undefined;
  return {
    read(account) {
      return account !== undefined && entry?.account === account ? entry.value : undefined;
    },
    write(account, value) {
      if (account !== undefined) {
        entry = { account, value };
      }
    },
  };
}
