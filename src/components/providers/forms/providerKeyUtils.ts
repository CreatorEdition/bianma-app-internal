export const PROVIDER_KEY_PATTERN = /^[a-z0-9]+(-[a-z0-9]+)*$/;

/**
 * 将供应商标识输入归一化为 OpenCode/OpenClaw 可接受的安全字符集。
 */
export function normalizeProviderKeyInput(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9-]/g, "");
}

/**
 * 校验供应商标识是否符合小写字母、数字与单个连字符分段格式。
 */
export function isProviderKeyValid(value: string): boolean {
  return PROVIDER_KEY_PATTERN.test(value);
}
