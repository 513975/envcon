const entries = new Map<
  string,
  { expires: number; promise: Promise<unknown> }
>();

export function invalidateReads() {
  entries.clear();
}

export function cachedRead<T>(
  key: string,
  read: () => Promise<T>,
  force = false,
): Promise<T> {
  const old = entries.get(key);
  if (!force && old && old.expires > Date.now())
    return old.promise as Promise<T>;
  const entry = {
    expires: Infinity,
    promise: Promise.resolve(null) as Promise<unknown>,
  };
  entry.promise = read().then(
    (value) => {
      entry.expires = Date.now() + 60_000;
      return value;
    },
    (error) => {
      if (entries.get(key) === entry) entries.delete(key);
      throw error;
    },
  );
  entries.set(key, entry);
  return entry.promise as Promise<T>;
}
