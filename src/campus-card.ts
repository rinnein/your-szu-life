/**
 * 校园卡号规则：固定 6 位数字。
 *
 * 输入框本身用 `maxlength` 与键入时的数字过滤挡住非法输入；校验器只负责把
 * 「超出 6 位」和「没有填满 6 位」显式标红，不做静默截断，否则用户会看到
 * 自己敲进去的数字被悄悄吞掉。
 */
export const CAMPUS_CARD_LENGTH = 6;

export const CAMPUS_CARD_ERROR = "校园卡号必须是 6 位数字";

/** 键入时只保留数字；长度由 maxlength 兜住，超出交给校验器提示。 */
export function campusCardDigits(value: string): string {
  return value.replace(/\D/g, "");
}

export function isCampusCardComplete(value: string): boolean {
  const card = value.trim();
  return card.length === CAMPUS_CARD_LENGTH && /^\d+$/.test(card);
}

/**
 * 超出位数立刻报错；位数不足只在提交过一次后提示，避免刚敲第一位就标红。
 */
export function campusCardError(value: string, submitted: boolean): string | undefined {
  if (value.trim().length > CAMPUS_CARD_LENGTH) return CAMPUS_CARD_ERROR;
  if (submitted && !isCampusCardComplete(value)) return CAMPUS_CARD_ERROR;
  return undefined;
}
