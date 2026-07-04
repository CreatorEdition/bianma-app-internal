type StorageLike = Pick<Storage, "getItem" | "setItem" | "removeItem">;

const resolveStorage = (storage?: StorageLike): StorageLike | null => {
  if (storage) {
    return storage;
  }

  if (typeof window === "undefined") {
    return null;
  }

  return window.localStorage;
};

const uniqueLegacyKeys = (primaryKey: string, legacyKeys: string[]): string[] =>
  Array.from(
    new Set(
      legacyKeys
        .map((key) => key.trim())
        .filter((key) => key.length > 0 && key !== primaryKey),
    ),
  );

export const readCompatibleStorage = (
  primaryKey: string,
  legacyKeys: string[] = [],
  storage?: StorageLike,
): string | null => {
  const targetStorage = resolveStorage(storage);
  if (!targetStorage) {
    return null;
  }

  const current = targetStorage.getItem(primaryKey);
  if (current !== null) {
    return current;
  }

  for (const legacyKey of uniqueLegacyKeys(primaryKey, legacyKeys)) {
    const legacyValue = targetStorage.getItem(legacyKey);
    if (legacyValue === null) {
      continue;
    }

    targetStorage.setItem(primaryKey, legacyValue);
    targetStorage.removeItem(legacyKey);
    return legacyValue;
  }

  return null;
};

export const writeCompatibleStorage = (
  primaryKey: string,
  value: string,
  legacyKeys: string[] = [],
  storage?: StorageLike,
): void => {
  const targetStorage = resolveStorage(storage);
  if (!targetStorage) {
    return;
  }

  targetStorage.setItem(primaryKey, value);
  for (const legacyKey of uniqueLegacyKeys(primaryKey, legacyKeys)) {
    targetStorage.removeItem(legacyKey);
  }
};

export const removeCompatibleStorage = (
  primaryKey: string,
  legacyKeys: string[] = [],
  storage?: StorageLike,
): void => {
  const targetStorage = resolveStorage(storage);
  if (!targetStorage) {
    return;
  }

  targetStorage.removeItem(primaryKey);
  for (const legacyKey of uniqueLegacyKeys(primaryKey, legacyKeys)) {
    targetStorage.removeItem(legacyKey);
  }
};

export const consumeLegacyStorage = (
  legacyKeys: string[] = [],
  storage?: StorageLike,
): string | null => {
  const targetStorage = resolveStorage(storage);
  if (!targetStorage) {
    return null;
  }

  for (const legacyKey of Array.from(
    new Set(legacyKeys.map((key) => key.trim())),
  )) {
    if (!legacyKey) {
      continue;
    }

    const legacyValue = targetStorage.getItem(legacyKey);
    if (legacyValue === null) {
      continue;
    }

    targetStorage.removeItem(legacyKey);
    return legacyValue;
  }

  return null;
};
