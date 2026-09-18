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
  entry.promise = Promise.resolve().then(read).then(
    (value) => {
      // A mutation or a newer refresh superseded this request while it was running.
      if (entries.get(key) !== entry) return cachedRead(key, read);
      entry.expires = Date.now() + 60_000;
      return value;
    },
    (error) => {
      if (entries.get(key) !== entry) return cachedRead(key, read);
      entries.delete(key);
      throw error;
    },
  );
  entries.set(key, entry);
  return entry.promise as Promise<T>;
}
